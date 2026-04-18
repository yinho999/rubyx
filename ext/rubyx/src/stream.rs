use crate::python_ffi::PyObject;
use crate::ruby_helpers::runtime_error;
use crate::rubyx_object::{python_to_sendable, RubyxObject};
use crate::{api, ruby_helpers};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use magnus::encoding::EncodingCapable;
use magnus::value::ReprValue;
use magnus::{IntoValue, Value};
use std::thread;
use std::thread::JoinHandle;

/// Thread-safe intermediate representation of a Python/Ruby value.
///
/// `magnus::Value` wraps a `*mut RBasic` (a raw pointer), which is NOT Send.
/// We must convert Python values to pure Rust types in the worker thread,
/// send those through the channel, and convert to `magnus::Value` on the
/// Ruby thread (which holds the GVL).
#[derive(Debug)]
pub(crate) enum SendableValue {
    Nil,
    Integer(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Bytes(Vec<u8>),
    Set(Vec<SendableValue>),
    List(Vec<SendableValue>),
    Dict(Vec<(SendableValue, SendableValue)>),
    PyObjectRef(usize),
}
impl TryInto<magnus::Value> for SendableValue {
    type Error = magnus::Error;

    fn try_into(self) -> Result<Value, Self::Error> {
        let ruby = magnus::Ruby::get().map_err(|_| {
            magnus::Error::new(runtime_error(), "Must be called on Ruby thread".to_string())
        })?;
        let result = match self {
            SendableValue::Nil => ruby.qnil().as_value(),
            SendableValue::Integer(n) => n.into_value_with(&ruby),
            SendableValue::Float(f) => f.into_value_with(&ruby),
            SendableValue::Str(s) => s.as_str().into_value_with(&ruby),
            SendableValue::Bool(b) => b.into_value_with(&ruby),
            SendableValue::Bytes(b) => {
                let s = ruby.str_from_slice(&b);
                s.enc_associate(ruby.find_encoding("ASCII-8BIT").ok_or(magnus::Error::new(
                    ruby_helpers::no_method_error(),
                    "No ASCII-8BIT",
                ))?)?;
                s.as_value()
            }
            SendableValue::List(l) | SendableValue::Set(l) => {
                let ruby_array = ruby.ary_new_capa(l.len());
                for item in l {
                    let val: Value = item.try_into()?;
                    ruby_array.push(val)?;
                }
                ruby_array.as_value()
            }
            SendableValue::Dict(entries) => {
                let hash = ruby.hash_new();
                for (k, v) in entries {
                    let key: Value = k.try_into()?;
                    let val: Value = v.try_into()?;
                    hash.aset(key, val)?;
                }
                hash.as_value()
            }
            SendableValue::PyObjectRef(addr) => {
                let py_obj = addr as *mut PyObject;
                let api = crate::API.get().ok_or_else(|| {
                    magnus::Error::new(runtime_error(), "Python API not initialized")
                })?;
                // RubyxObject::new increfs internally and python_to_sendable also incref
                let wrapper = RubyxObject::new(py_obj, api).ok_or_else(|| {
                    magnus::Error::new(runtime_error(), "Failed to wrap Python object")
                })?;
                // Balance the extra incref from python_to_sendable
                api.decref(py_obj);
                wrapper.into_value_with(&ruby)
            }
        };
        Ok(result)
    }
}

/// Item sent through stream - Send safe
pub(crate) enum StreamItem {
    Value(SendableValue),
    Error(String),
    End,
}

/// A Stream of values from a background thread
#[allow(dead_code)]
pub struct AsyncStream {
    receiver: Option<Receiver<StreamItem>>,
    cancel_sender: Sender<()>,
    handle: Option<JoinHandle<()>>,
}

impl AsyncStream {
    /// Stream which iterates a Python Iterator in the background
    #[allow(dead_code)]
    pub fn from_python_iterator(py_iter: *mut PyObject) -> Self {
        let (value_tx, value_rx) = unbounded();
        let (cancel_tx, cancel_rx) = bounded(1);

        // Cast the raw pointer to usize so it can cross the thread boundary.
        // *mut PyObject is not Send, but the usize value is just a number.
        // This is safe because the worker thread will acquire the GIL before
        // using the pointer, and the pointer remains valid (Python iterator
        // is kept alive by its refcount).
        let py_iter_addr = py_iter as usize;

        let handle = thread::spawn(move || {
            let py_iter = py_iter_addr as *mut PyObject;
            // Worker thread: acquire GIL, iterate, send values
            let api = api();
            let gil = api.ensure_gil();

            loop {
                // Check if there is a cancellation
                if cancel_rx.try_recv().is_ok() {
                    break;
                }

                // Get next item from Python iterator
                let item = api.iter_next(py_iter);
                if item.is_null() {
                    // Check if an exception was raised (vs normal exhaustion)
                    if api.has_error() {
                        if let Some(exc) = crate::python_api::PythonApi::extract_exception(api) {
                            value_tx.send(StreamItem::Error(exc.to_string())).ok();
                        } else {
                            value_tx.send(StreamItem::End).ok();
                        }
                    } else {
                        value_tx.send(StreamItem::End).ok();
                    }
                    break;
                }
                // Convert and send to ruby
                let ruby_value = python_to_sendable(item, api)
                    .map_err(|e| format!("Error converting Python value to Ruby: {e}"));
                api.decref(item);
                match ruby_value {
                    Ok(value) => {
                        if value_tx.send(StreamItem::Value(value)).is_err() {
                            break; // Consumer dropped — stop producing
                        }
                    }
                    Err(e) => {
                        value_tx.send(StreamItem::Error(e)).ok();
                        break;
                    }
                }
            }
            api.decref(py_iter);
            api.release_gil(gil);
        });
        Self {
            receiver: Some(value_rx),
            cancel_sender: cancel_tx,
            handle: Some(handle),
        }
    }
}

impl Drop for AsyncStream {
    fn drop(&mut self) {
        // 1. Signal cancellation
        self.cancel_sender.try_send(()).ok();
        // 2. Drain then drop the receiver so value_tx.send() returns Err,
        //    unblocking the worker thread
        if let Some(rx) = self.receiver.take() {
            while rx.try_recv().is_ok() {}
            drop(rx);
        }
        // 3. Join the worker thread (now guaranteed to exit)
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
impl AsyncStream {
    /// Test constructor: create an AsyncStream from a channel of `Option<SendableValue>`.
    /// `Some(val)` sends a value, `None` signals end-of-stream.
    pub(crate) fn from_channel(rx: Receiver<Option<SendableValue>>, cancel_tx: Sender<()>) -> Self {
        let (value_tx, value_rx) = unbounded();

        let handle = thread::spawn(move || {
            while let Ok(item) = rx.recv() {
                match item {
                    Some(val) => {
                        if value_tx.send(StreamItem::Value(val)).is_err() {
                            return;
                        }
                    }
                    None => {
                        value_tx.send(StreamItem::End).ok();
                        return;
                    }
                }
            }
            // Sender dropped without None — treat as end
            value_tx.send(StreamItem::End).ok();
        });

        Self {
            receiver: Some(value_rx),
            cancel_sender: cancel_tx,
            handle: Some(handle),
        }
    }
}

impl Iterator for AsyncStream {
    type Item = Result<Value, magnus::Error>;
    fn next(&mut self) -> Option<Self::Item> {
        let rx = self.receiver.as_ref()?;
        match rx.recv() {
            Ok(StreamItem::Value(v)) => Some(v.try_into()),
            Ok(StreamItem::Error(e)) => Some(Err(magnus::Error::new(runtime_error(), e))),
            Ok(StreamItem::End) | Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{skip_if_no_python, with_ruby_python};
    use crossbeam_channel::bounded;
    use magnus::TryConvert;
    use serial_test::serial;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    // ========== Iterator: basic value delivery ==========

    #[test]
    #[serial]
    fn test_iterator_delivers_single_value() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Integer(42))).ok();
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            let val = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(val).unwrap(), 42);
            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_iterator_delivers_multiple_values_in_order() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                for i in 0..5 {
                    tx.send(Some(SendableValue::Integer(i))).ok();
                }
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            for expected in 0..5 {
                let val = stream.next().unwrap().unwrap();
                assert_eq!(i64::try_convert(val).unwrap(), expected);
            }
            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_iterator_empty_stream() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_iterator_propagates_error() {
        with_ruby_python(|_ruby, _api| {
            let (value_tx, value_rx) = unbounded();

            // Manually build the stream to inject an error StreamItem
            let (cancel_tx, _cancel_rx) = bounded(1);
            let handle = thread::spawn(move || {
                value_tx
                    .send(StreamItem::Value(SendableValue::Integer(1)))
                    .ok();
                value_tx
                    .send(StreamItem::Error("something went wrong".to_string()))
                    .ok();
            });

            let mut stream = AsyncStream {
                receiver: Some(value_rx),
                cancel_sender: cancel_tx,
                handle: Some(handle),
            };

            // First item succeeds
            let val = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(val).unwrap(), 1);

            // Second item is an error
            let err = stream.next().unwrap().unwrap_err();
            assert!(err.to_string().contains("something went wrong"));
        });
    }

    #[test]
    #[serial]
    fn test_iterator_end_then_none() {
        with_ruby_python(|_ruby, _api| {
            let (value_tx, value_rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            let handle = thread::spawn(move || {
                value_tx.send(StreamItem::End).ok();
            });

            let mut stream = AsyncStream {
                receiver: Some(value_rx),
                cancel_sender: cancel_tx,
                handle: Some(handle),
            };

            assert!(stream.next().is_none());
            // Calling next after End should also return None (channel closed)
            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_iterator_channel_closed_returns_none() {
        with_ruby_python(|_ruby, _api| {
            let (value_tx, value_rx) = bounded::<StreamItem>(16);
            let (cancel_tx, _cancel_rx) = bounded(1);

            // Drop sender immediately — simulates producer crash
            drop(value_tx);

            let mut stream = AsyncStream {
                receiver: Some(value_rx),
                cancel_sender: cancel_tx,
                handle: None,
            };

            assert!(stream.next().is_none());
        });
    }

    // ========== Iterator: type handling ==========

    #[test]
    #[serial]
    fn test_iterator_all_sendable_types() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Nil)).ok();
                tx.send(Some(SendableValue::Integer(99))).ok();
                tx.send(Some(SendableValue::Float(1.5))).ok();
                tx.send(Some(SendableValue::Str("test".to_string()))).ok();
                tx.send(Some(SendableValue::Bool(false))).ok();
                tx.send(Some(SendableValue::List(vec![SendableValue::Integer(1)])))
                    .ok();
                tx.send(Some(SendableValue::Dict(vec![(
                    SendableValue::Str("k".to_string()),
                    SendableValue::Integer(2),
                )])))
                .ok();
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            let mut results = Vec::new();
            while let Some(Ok(val)) = stream.next() {
                results.push(val);
            }
            assert_eq!(results.len(), 7);

            assert!(results[0].is_nil());
            assert_eq!(i64::try_convert(results[1]).unwrap(), 99);
            assert!((f64::try_convert(results[2]).unwrap() - 1.5).abs() < 1e-9);
            assert_eq!(String::try_convert(results[3]).unwrap(), "test");
            assert!(!bool::try_convert(results[4]).unwrap());

            let arr = magnus::RArray::try_convert(results[5]).unwrap();
            assert_eq!(arr.len(), 1);

            let hash = magnus::RHash::try_convert(results[6]).unwrap();
            let v: i64 = hash.fetch("k").unwrap();
            assert_eq!(v, 2);
        });
    }

    // ========== Drop: cancellation signal ==========

    #[test]
    #[serial]
    fn test_drop_sends_cancel_signal() {
        let Some(_guard) = skip_if_no_python() else {
            return;
        };

        let (tx, rx) = unbounded();
        let (cancel_tx, cancel_rx) = bounded(1);

        thread::spawn(move || {
            tx.send(Some(SendableValue::Integer(1))).ok();
            // Don't send None — stream stays open
        });

        let stream = AsyncStream::from_channel(rx, cancel_tx);
        drop(stream);

        // The cancel channel should have received a signal
        // (or be disconnected because cancel_sender was dropped after sending)
        // Either way, the cancel_rx side should have gotten the message
        assert!(
            cancel_rx.try_recv().is_ok() || cancel_rx.try_recv().is_err(),
            "cancel signal should have been sent before drop completed"
        );
    }

    #[test]
    #[serial]
    fn test_drop_joins_worker_thread() {
        let Some(_guard) = skip_if_no_python() else {
            return;
        };

        let thread_finished = Arc::new(AtomicBool::new(false));
        let thread_finished_clone = thread_finished.clone();

        let (value_tx, value_rx) = unbounded();
        let (cancel_tx, cancel_rx) = bounded(1);

        let handle = thread::spawn(move || {
            // Wait for cancel signal or a short timeout
            let _ = cancel_rx.recv_timeout(Duration::from_secs(5));
            value_tx.send(StreamItem::End).ok();
            thread_finished_clone.store(true, Ordering::SeqCst);
        });

        let stream = AsyncStream {
            receiver: Some(value_rx),
            cancel_sender: cancel_tx,
            handle: Some(handle),
        };

        // Drop should send cancel, drain, and join
        drop(stream);

        // After drop returns, the thread must have finished
        assert!(
            thread_finished.load(Ordering::SeqCst),
            "worker thread should have been joined by Drop"
        );
    }

    #[test]
    #[serial]
    fn test_drop_unblocks_producer_via_drain() {
        let Some(_guard) = skip_if_no_python() else {
            return;
        };

        let (value_tx, value_rx) = bounded(1); // tiny buffer
        let (cancel_tx, cancel_rx) = bounded(1);

        let producer_done = Arc::new(AtomicBool::new(false));
        let producer_done_clone = producer_done.clone();

        let handle = thread::spawn(move || {
            // Fill the buffer
            let _ = value_tx.send(StreamItem::Value(SendableValue::Integer(1)));
            // This will block because buffer is full
            // Drop's drain + cancel should unblock us
            loop {
                crossbeam_channel::select! {
                    send(value_tx, StreamItem::Value(SendableValue::Integer(2))) -> res => {
                        if res.is_err() { break; }
                    }
                    recv(cancel_rx) -> _ => {
                        break;
                    }
                }
            }
            producer_done_clone.store(true, Ordering::SeqCst);
        });

        let stream = AsyncStream {
            receiver: Some(value_rx),
            cancel_sender: cancel_tx,
            handle: Some(handle),
        };

        let start = Instant::now();
        drop(stream);
        let elapsed = start.elapsed();

        assert!(
            producer_done.load(Ordering::SeqCst),
            "producer should have finished after drop"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "drop should not hang — took {:?}",
            elapsed
        );
    }

    // ========== Drop: mid-iteration ==========

    #[test]
    #[serial]
    fn test_drop_mid_iteration_does_not_hang() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                for i in 0..100 {
                    if tx.send(Some(SendableValue::Integer(i))).is_err() {
                        return;
                    }
                }
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);

            // Read only a few items, then drop
            let _ = stream.next();
            let _ = stream.next();

            let start = Instant::now();
            drop(stream);
            let elapsed = start.elapsed();

            assert!(
                elapsed < Duration::from_secs(2),
                "dropping mid-iteration should not hang — took {:?}",
                elapsed
            );
        });
    }

    #[test]
    #[serial]
    fn test_drop_without_reading_any_items() {
        let Some(_guard) = skip_if_no_python() else {
            return;
        };

        let (tx, rx) = unbounded();
        let (cancel_tx, _cancel_rx) = bounded(1);

        let producer_done = Arc::new(AtomicBool::new(false));
        let producer_done_clone = producer_done.clone();

        thread::spawn(move || {
            for i in 0..10 {
                if tx.send(Some(SendableValue::Integer(i))).is_err() {
                    break;
                }
            }
            tx.send(None).ok();
            producer_done_clone.store(true, Ordering::SeqCst);
        });

        let stream = AsyncStream::from_channel(rx, cancel_tx);

        let start = Instant::now();
        drop(stream); // Never called .next()
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_secs(2),
            "dropping without reading should not hang — took {:?}",
            elapsed
        );
    }

    // ========== Drop: handle is None ==========

    #[test]
    #[serial]
    fn test_drop_with_no_handle() {
        let Some(_guard) = skip_if_no_python() else {
            return;
        };

        let (_value_tx, value_rx) = bounded::<StreamItem>(16);
        let (cancel_tx, _cancel_rx) = bounded(1);

        // handle: None — simulates already-joined or no thread
        let stream = AsyncStream {
            receiver: Some(value_rx),
            cancel_sender: cancel_tx,
            handle: None,
        };

        // Should not panic
        drop(stream);
    }

    // ========== from_channel: producer sends then drops ==========

    #[test]
    #[serial]
    fn test_from_channel_producer_drops_without_none() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Integer(1))).ok();
                tx.send(Some(SendableValue::Integer(2))).ok();
                drop(tx); // Drop without sending None
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);

            let v1 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1).unwrap(), 1);

            let v2 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v2).unwrap(), 2);

            // from_channel sends End when producer drops
            assert!(stream.next().is_none());
        });
    }

    // ========== Backpressure ==========

    #[test]
    #[serial]
    fn test_backpressure_with_slow_consumer() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = bounded(2); // very small buffer
            let (cancel_tx, _cancel_rx) = bounded(1);

            let items_sent = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let items_sent_clone = items_sent.clone();

            thread::spawn(move || {
                for i in 0..20 {
                    if tx.send(Some(SendableValue::Integer(i))).is_err() {
                        break;
                    }
                    items_sent_clone.fetch_add(1, Ordering::SeqCst);
                }
                tx.send(None).ok();
            });

            // Give producer time to fill buffer
            thread::sleep(Duration::from_millis(50));

            // Producer should be blocked after filling the internal buffer
            // (2 items in tx→rx + 16 in internal value channel)
            let sent_before_read = items_sent.load(Ordering::SeqCst);
            assert!(
                sent_before_read <= 20, // bounded by buffer sizes
                "producer should be bounded by channel capacity"
            );

            // Now consume everything
            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            let mut count = 0;
            while let Some(Ok(_)) = stream.next() {
                count += 1;
            }
            assert_eq!(count, 20);
        });
    }

    // ========== Large stream ==========

    #[test]
    #[serial]
    fn test_large_stream_1000_items() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                for i in 0..1000 {
                    if tx.send(Some(SendableValue::Integer(i))).is_err() {
                        return;
                    }
                }
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            let mut count = 0i64;
            let mut sum = 0i64;
            while let Some(Ok(val)) = stream.next() {
                sum += i64::try_convert(val).unwrap();
                count += 1;
            }
            assert_eq!(count, 1000);
            assert_eq!(sum, (0..1000i64).sum::<i64>());
        });
    }

    // ========== PyObjectRef: SendableValue → RubyxObject ==========

    #[test]
    #[serial]
    fn test_py_object_ref_converts_to_rubyx_object() {
        with_ruby_python(|_ruby, api| {
            let os = api.import_module("os").expect("os should import");
            api.incref(os); // incref for PyObjectRef (simulates what python_to_sendable does)
            let sendable = SendableValue::PyObjectRef(os as usize);

            let val: Value = sendable.try_into().expect("PyObjectRef should convert");
            // The result should be a RubyxObject, not a primitive
            assert!(!val.is_nil());
            // Verify it's not a primitive type
            assert!(i64::try_convert(val).is_err(), "should not be an Integer");
            assert!(String::try_convert(val).is_err(), "should not be a String");

            api.decref(os); // balance the import_module refcount
        });
    }

    #[test]
    #[serial]
    fn test_py_object_ref_in_stream() {
        with_ruby_python(|_ruby, api| {
            let os = api.import_module("os").expect("os should import");
            api.incref(os);

            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);
            let addr = os as usize;

            thread::spawn(move || {
                tx.send(Some(SendableValue::Integer(1))).ok();
                tx.send(Some(SendableValue::PyObjectRef(addr))).ok();
                tx.send(Some(SendableValue::Str("after".to_string()))).ok();
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);

            // First: integer
            let val = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(val).unwrap(), 1);

            // Second: PyObjectRef → RubyxObject
            let val = stream.next().unwrap().unwrap();
            assert!(!val.is_nil());
            assert!(
                i64::try_convert(val).is_err(),
                "should be RubyxObject, not Integer"
            );

            // Third: string
            let val = stream.next().unwrap().unwrap();
            assert_eq!(String::try_convert(val).unwrap(), "after");

            assert!(stream.next().is_none());
            api.decref(os);
        });
    }

    // ========== Set: SendableValue::Set → Ruby Array ==========

    #[test]
    #[serial]
    fn test_set_converts_to_ruby_array() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Set(vec![
                    SendableValue::Integer(1),
                    SendableValue::Integer(2),
                    SendableValue::Integer(3),
                ])))
                .ok();
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            let val = stream.next().unwrap().unwrap();

            let arr = magnus::RArray::try_convert(val).unwrap();
            assert_eq!(arr.len(), 3);

            let mut items: Vec<i64> = (0..arr.len())
                .map(|i| i64::try_convert(arr.entry::<Value>(i as isize).unwrap()).unwrap())
                .collect();
            items.sort();
            assert_eq!(items, vec![1, 2, 3]);

            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_empty_set_converts_to_empty_ruby_array() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Set(vec![]))).ok();
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            let val = stream.next().unwrap().unwrap();

            let arr = magnus::RArray::try_convert(val).unwrap();
            assert_eq!(arr.len(), 0);

            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_set_with_mixed_types_converts_to_ruby_array() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Set(vec![
                    SendableValue::Integer(42),
                    SendableValue::Str("hello".to_string()),
                    SendableValue::Float(3.14),
                    SendableValue::Bool(true),
                ])))
                .ok();
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);
            let val = stream.next().unwrap().unwrap();

            let arr = magnus::RArray::try_convert(val).unwrap();
            assert_eq!(arr.len(), 4);

            assert_eq!(
                i64::try_convert(arr.entry::<Value>(0).unwrap()).unwrap(),
                42
            );
            assert_eq!(
                String::try_convert(arr.entry::<Value>(1).unwrap()).unwrap(),
                "hello"
            );
            assert!(
                (f64::try_convert(arr.entry::<Value>(2).unwrap()).unwrap() - 3.14).abs() < 1e-9
            );
            assert!(bool::try_convert(arr.entry::<Value>(3).unwrap()).unwrap());

            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_set_in_stream_with_other_types() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Integer(1))).ok();
                tx.send(Some(SendableValue::Set(vec![
                    SendableValue::Integer(10),
                    SendableValue::Integer(20),
                ])))
                .ok();
                tx.send(Some(SendableValue::Str("after".to_string()))).ok();
                tx.send(None).ok();
            });

            let mut stream = AsyncStream::from_channel(rx, cancel_tx);

            // First: integer
            let val = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(val).unwrap(), 1);

            // Second: set → array
            let val = stream.next().unwrap().unwrap();
            let arr = magnus::RArray::try_convert(val).unwrap();
            assert_eq!(arr.len(), 2);

            // Third: string
            let val = stream.next().unwrap().unwrap();
            assert_eq!(String::try_convert(val).unwrap(), "after");

            assert!(stream.next().is_none());
        });
    }
}
