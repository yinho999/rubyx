use crate::async_gen::AsyncGeneratorStream;
use crate::ruby_helpers::runtime_error;
use magnus::value::ReprValue;
use magnus::{Error, Ruby, Value};

#[magnus::wrap(class = "Rubyx::Stream", free_immediately)]
pub(crate) struct RubyxStream {
    inner: std::cell::RefCell<Option<AsyncGeneratorStream>>,
}

impl RubyxStream {
    pub fn each(&self) -> Result<Value, magnus::Error> {
        let ruby = Ruby::get()
            .map_err(|e| magnus::Error::new(runtime_error(), format!("Error getting Ruby: {e}")))?;

        // Enumerator
        if !ruby.block_given() {
            let receiver: Value = ruby.current_receiver()?;
            return Ok(receiver.enumeratorize("each", ()).as_value());
        }

        // Stream
        // Take ownership of the stream so it gets dropped when `each` returns.
        // This is critical: when Ruby's `first`/`take` break out of `each` early,
        // the stream must be cleaned up immediately (cancel + join worker thread)
        // rather than waiting for Ruby's GC. Otherwise the worker thread holds
        // the Python GIL and subsequent Python calls deadlock.
        let mut stream = self
            .inner
            .borrow_mut()
            .take()
            .ok_or_else(|| magnus::Error::new(runtime_error(), "Stream already consumed"))?;
        for result in &mut stream {
            match result {
                Ok(val) => {
                    let _: Value = ruby.yield_value(val)?;
                }
                Err(err) => return Err(err),
            }
        }
        Ok(ruby.qnil().as_value())
    }

    pub fn next_item(&self) -> Result<Value, Error> {
        let ruby = Ruby::get()
            .map_err(|e| magnus::Error::new(runtime_error(), format!("Error getting Ruby: {e}")))?;
        let mut inner = self.inner.borrow_mut();
        let stream = inner
            .as_mut()
            .ok_or_else(|| magnus::Error::new(runtime_error(), "Stream already consumed"))?;
        match stream.next() {
            Some(Ok(val)) => Ok(val),
            Some(Err(err)) => Err(err),
            None => {
                // Stream exhausted — take it out so it gets dropped and cleaned up
                inner.take();
                Err(magnus::Error::new(
                    ruby.exception_stop_iteration(),
                    "iteration reached an end",
                ))
            }
        }
    }
    pub fn from_stream(stream: AsyncGeneratorStream) -> Self {
        Self {
            inner: std::cell::RefCell::new(Some(stream)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::stream::SendableValue;
    use crate::test_helpers::{skip_if_no_python, with_ruby_python};
    use crossbeam_channel::{bounded, unbounded};
    use magnus::encoding::EncodingCapable;
    use magnus::value::ReprValue;
    use magnus::TryConvert;
    use serial_test::serial;
    use std::thread;

    // ========== SendableValue → magnus::Value conversion ==========

    #[test]
    #[serial]
    fn test_sendable_nil_converts_to_ruby_nil() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Nil
                .try_into()
                .expect("Nil conversion should succeed");
            assert!(
                val.is_nil(),
                "SendableValue::Nil should convert to Ruby nil"
            );
        });
    }

    #[test]
    #[serial]
    fn test_sendable_integer_converts_to_ruby_integer() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Integer(42)
                .try_into()
                .expect("Integer conversion should succeed");
            let n = i64::try_convert(val).expect("should be convertible to i64");
            assert_eq!(n, 42);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_integer_negative() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Integer(-99)
                .try_into()
                .expect("negative integer conversion should succeed");
            let n = i64::try_convert(val).expect("should be convertible to i64");
            assert_eq!(n, -99);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_integer_zero() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Integer(0)
                .try_into()
                .expect("zero conversion should succeed");
            let n = i64::try_convert(val).expect("should be convertible to i64");
            assert_eq!(n, 0);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_float_converts_to_ruby_float() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Float(std::f64::consts::PI)
                .try_into()
                .expect("Float conversion should succeed");
            let f = f64::try_convert(val).expect("should be convertible to f64");
            assert!((f - std::f64::consts::PI).abs() < 1e-9);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_float_negative() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Float(-0.5)
                .try_into()
                .expect("negative float conversion should succeed");
            let f = f64::try_convert(val).expect("should be convertible to f64");
            assert!((f - (-0.5)).abs() < 1e-9);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_string_converts_to_ruby_string() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Str("hello world".to_string())
                .try_into()
                .expect("String conversion should succeed");
            let s = String::try_convert(val).expect("should be convertible to String");
            assert_eq!(s, "hello world");
        });
    }

    #[test]
    #[serial]
    fn test_sendable_string_empty() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Str(String::new())
                .try_into()
                .expect("empty string conversion should succeed");
            let s = String::try_convert(val).expect("should be convertible to String");
            assert_eq!(s, "");
        });
    }

    #[test]
    #[serial]
    fn test_sendable_string_unicode() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Str("こんにちは🌍".to_string())
                .try_into()
                .expect("unicode string conversion should succeed");
            let s = String::try_convert(val).expect("should be convertible to String");
            assert_eq!(s, "こんにちは🌍");
        });
    }

    #[test]
    #[serial]
    fn test_sendable_bool_true() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Bool(true)
                .try_into()
                .expect("Bool(true) conversion should succeed");
            let b = bool::try_convert(val).expect("should be convertible to bool");
            assert!(b);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_bool_false() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Bool(false)
                .try_into()
                .expect("Bool(false) conversion should succeed");
            let b = bool::try_convert(val).expect("should be convertible to bool");
            assert!(!b);
        });
    }

    // ========== SendableValue::List → Ruby Array ==========

    #[test]
    #[serial]
    fn test_sendable_list_converts_to_ruby_array() {
        with_ruby_python(|_ruby, _api| {
            let list = SendableValue::List(vec![
                SendableValue::Integer(1),
                SendableValue::Integer(2),
                SendableValue::Integer(3),
            ]);
            let val: magnus::Value = list.try_into().expect("List conversion should succeed");

            assert!(val.is_kind_of(_ruby.class_array()));
            let arr = magnus::RArray::try_convert(val).expect("should be an Array");
            assert_eq!(arr.len(), 3);

            let items: Vec<i64> = (0..3)
                .map(|i| i64::try_convert(arr.entry::<magnus::Value>(i).unwrap()).unwrap())
                .collect();
            assert_eq!(items, vec![1, 2, 3]);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_list_empty() {
        with_ruby_python(|_ruby, _api| {
            let list = SendableValue::List(vec![]);
            let val: magnus::Value = list
                .try_into()
                .expect("empty list conversion should succeed");
            let arr = magnus::RArray::try_convert(val).expect("should be an Array");
            assert_eq!(arr.len(), 0);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_list_mixed_types() {
        with_ruby_python(|_ruby, _api| {
            let list = SendableValue::List(vec![
                SendableValue::Integer(42),
                SendableValue::Str("hello".to_string()),
                SendableValue::Float(2.5),
                SendableValue::Bool(true),
                SendableValue::Nil,
            ]);
            let val: magnus::Value = list
                .try_into()
                .expect("mixed list conversion should succeed");

            let arr = magnus::RArray::try_convert(val).expect("should be an Array");
            assert_eq!(arr.len(), 5);

            let v0 = arr.entry::<magnus::Value>(0).unwrap();
            let v1 = arr.entry::<magnus::Value>(1).unwrap();
            let v2 = arr.entry::<magnus::Value>(2).unwrap();
            let v3 = arr.entry::<magnus::Value>(3).unwrap();
            let v4 = arr.entry::<magnus::Value>(4).unwrap();

            assert_eq!(i64::try_convert(v0).unwrap(), 42);
            assert_eq!(String::try_convert(v1).unwrap(), "hello");
            assert!((f64::try_convert(v2).unwrap() - 2.5).abs() < 1e-9);
            assert!(bool::try_convert(v3).unwrap());
            assert!(v4.is_nil());
        });
    }

    #[test]
    #[serial]
    fn test_sendable_list_nested() {
        with_ruby_python(|_ruby, _api| {
            let nested = SendableValue::List(vec![
                SendableValue::List(vec![SendableValue::Integer(1), SendableValue::Integer(2)]),
                SendableValue::List(vec![SendableValue::Integer(3), SendableValue::Integer(4)]),
            ]);
            let val: magnus::Value = nested
                .try_into()
                .expect("nested list conversion should succeed");

            let outer = magnus::RArray::try_convert(val).expect("should be an Array");
            assert_eq!(outer.len(), 2);

            let inner0 = magnus::RArray::try_convert(outer.entry::<magnus::Value>(0).unwrap())
                .expect("inner should be an Array");
            assert_eq!(inner0.len(), 2);
            assert_eq!(
                i64::try_convert(inner0.entry::<magnus::Value>(0).unwrap()).unwrap(),
                1
            );
            assert_eq!(
                i64::try_convert(inner0.entry::<magnus::Value>(1).unwrap()).unwrap(),
                2
            );

            let inner1 = magnus::RArray::try_convert(outer.entry::<magnus::Value>(1).unwrap())
                .expect("inner should be an Array");
            assert_eq!(
                i64::try_convert(inner1.entry::<magnus::Value>(0).unwrap()).unwrap(),
                3
            );
            assert_eq!(
                i64::try_convert(inner1.entry::<magnus::Value>(1).unwrap()).unwrap(),
                4
            );
        });
    }

    // ========== SendableValue::Dict → Ruby Hash ==========

    #[test]
    #[serial]
    fn test_sendable_dict_converts_to_ruby_hash() {
        with_ruby_python(|_ruby, _api| {
            let dict = SendableValue::Dict(vec![
                (
                    SendableValue::Str("name".to_string()),
                    SendableValue::Str("Alice".to_string()),
                ),
                (
                    SendableValue::Str("age".to_string()),
                    SendableValue::Integer(30),
                ),
            ]);
            let val: magnus::Value = dict.try_into().expect("Dict conversion should succeed");

            assert!(val.is_kind_of(_ruby.class_hash()));
            let hash = magnus::RHash::try_convert(val).expect("should be a Hash");

            let name: String = hash.fetch("name").expect("should have 'name' key");
            assert_eq!(name, "Alice");
            let age: i64 = hash.fetch("age").expect("should have 'age' key");
            assert_eq!(age, 30);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_dict_empty() {
        with_ruby_python(|_ruby, _api| {
            let dict = SendableValue::Dict(vec![]);
            let val: magnus::Value = dict
                .try_into()
                .expect("empty dict conversion should succeed");
            let hash = magnus::RHash::try_convert(val).expect("should be a Hash");
            assert_eq!(hash.len(), 0);
        });
    }

    #[test]
    #[serial]
    fn test_sendable_dict_with_integer_keys() {
        with_ruby_python(|_ruby, _api| {
            let dict = SendableValue::Dict(vec![
                (
                    SendableValue::Integer(1),
                    SendableValue::Str("one".to_string()),
                ),
                (
                    SendableValue::Integer(2),
                    SendableValue::Str("two".to_string()),
                ),
            ]);
            let val: magnus::Value = dict
                .try_into()
                .expect("Dict with int keys conversion should succeed");

            let hash = magnus::RHash::try_convert(val).expect("should be a Hash");
            let v1: String = hash.fetch(1_i64).expect("should have key 1");
            assert_eq!(v1, "one");
            let v2: String = hash.fetch(2_i64).expect("should have key 2");
            assert_eq!(v2, "two");
        });
    }

    #[test]
    #[serial]
    fn test_sendable_dict_nested_list_value() {
        with_ruby_python(|_ruby, _api| {
            let dict = SendableValue::Dict(vec![(
                SendableValue::Str("data".to_string()),
                SendableValue::List(vec![SendableValue::Integer(10), SendableValue::Integer(20)]),
            )]);
            let val: magnus::Value = dict
                .try_into()
                .expect("Dict with list value conversion should succeed");

            let hash = magnus::RHash::try_convert(val).expect("should be a Hash");
            let data: magnus::Value = hash.fetch("data").expect("should have 'data' key");
            let arr = magnus::RArray::try_convert(data).expect("value should be an Array");
            assert_eq!(arr.len(), 2);
            assert_eq!(
                i64::try_convert(arr.entry::<magnus::Value>(0).unwrap()).unwrap(),
                10
            );
            assert_eq!(
                i64::try_convert(arr.entry::<magnus::Value>(1).unwrap()).unwrap(),
                20
            );
        });
    }

    // ========== AsyncStream as Iterator (via channel) ==========

    #[test]
    #[serial]
    fn test_async_stream_iterates_values() {
        with_ruby_python(|_ruby, _api| {
            let (value_tx, value_rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                value_tx.send(Some(SendableValue::Integer(10))).ok();
                value_tx.send(Some(SendableValue::Integer(20))).ok();
                value_tx.send(Some(SendableValue::Integer(30))).ok();
                value_tx.send(None).ok(); // End signal
            });

            let mut stream = crate::stream::AsyncStream::from_channel(value_rx, cancel_tx);

            let v1 = stream.next().unwrap().unwrap();
            let v2 = stream.next().unwrap().unwrap();
            let v3 = stream.next().unwrap().unwrap();
            assert!(stream.next().is_none());

            assert_eq!(i64::try_convert(v1).unwrap(), 10);
            assert_eq!(i64::try_convert(v2).unwrap(), 20);
            assert_eq!(i64::try_convert(v3).unwrap(), 30);
        });
    }

    #[test]
    #[serial]
    fn test_async_stream_empty() {
        with_ruby_python(|_ruby, _api| {
            let (value_tx, value_rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                value_tx.send(None).ok(); // Immediate end
            });

            let mut stream = crate::stream::AsyncStream::from_channel(value_rx, cancel_tx);
            assert!(
                stream.next().is_none(),
                "Empty stream should return None immediately"
            );
        });
    }

    #[test]
    #[serial]
    fn test_async_stream_mixed_types() {
        with_ruby_python(|_ruby, _api| {
            let (value_tx, value_rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                value_tx.send(Some(SendableValue::Integer(1))).ok();
                value_tx
                    .send(Some(SendableValue::Str("hello".to_string())))
                    .ok();
                value_tx.send(Some(SendableValue::Float(2.5))).ok();
                value_tx.send(Some(SendableValue::Bool(true))).ok();
                value_tx.send(Some(SendableValue::Nil)).ok();
                value_tx.send(None).ok();
            });

            let mut stream = crate::stream::AsyncStream::from_channel(value_rx, cancel_tx);
            let mut results = Vec::new();
            while let Some(Ok(val)) = stream.next() {
                results.push(val);
            }
            assert_eq!(results.len(), 5);

            assert_eq!(i64::try_convert(results[0]).unwrap(), 1);
            assert_eq!(String::try_convert(results[1]).unwrap(), "hello");
            assert!((f64::try_convert(results[2]).unwrap() - 2.5).abs() < 1e-9);
            assert!(bool::try_convert(results[3]).unwrap());
            assert!(results[4].is_nil());
        });
    }

    #[test]
    #[serial]
    fn test_async_stream_with_collections() {
        with_ruby_python(|_ruby, _api| {
            let (value_tx, value_rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                value_tx
                    .send(Some(SendableValue::List(vec![
                        SendableValue::Integer(1),
                        SendableValue::Integer(2),
                    ])))
                    .ok();
                value_tx
                    .send(Some(SendableValue::Dict(vec![(
                        SendableValue::Str("key".to_string()),
                        SendableValue::Str("val".to_string()),
                    )])))
                    .ok();
                value_tx.send(None).ok();
            });

            let mut stream = crate::stream::AsyncStream::from_channel(value_rx, cancel_tx);

            let arr_val = stream.next().unwrap().unwrap();
            let arr = magnus::RArray::try_convert(arr_val).expect("should be Array");
            assert_eq!(arr.len(), 2);

            let hash_val = stream.next().unwrap().unwrap();
            let hash = magnus::RHash::try_convert(hash_val).expect("should be Hash");
            let v: String = hash.fetch("key").unwrap();
            assert_eq!(v, "val");

            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_async_stream_cancellation_via_drop() {
        // Verify that dropping the stream doesn't hang even with a blocked producer
        let Some(_guard) = skip_if_no_python() else {
            return;
        };

        let (value_tx, value_rx) = bounded(1); // small buffer
        let (cancel_tx, cancel_rx) = bounded(1);

        let producer = thread::spawn(move || {
            // Fill the buffer
            let _ = value_tx.send(Some(SendableValue::Integer(1)));
            // This send will block because buffer is full and consumer won't read
            // The cancel signal or channel close should unblock us
            loop {
                crossbeam_channel::select! {
                    send(value_tx, Some(SendableValue::Integer(2))) -> res => {
                        if res.is_err() { break; } // channel closed
                    }
                    recv(cancel_rx) -> _ => {
                        break; // cancelled
                    }
                }
            }
        });

        // Drop the stream (sends cancel, drops receiver)
        let stream = crate::stream::AsyncStream::from_channel(value_rx, cancel_tx);
        drop(stream);

        // Producer thread should finish without hanging
        producer
            .join()
            .expect("producer should not hang after cancellation");
    }

    #[test]
    #[serial]
    fn test_async_stream_channel_closed_returns_none() {
        with_ruby_python(|_ruby, _api| {
            let (value_tx, value_rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            // Drop sender immediately — simulates producer crash
            drop(value_tx);

            let mut stream = crate::stream::AsyncStream::from_channel(value_rx, cancel_tx);
            assert!(stream.next().is_none(), "Closed channel should return None");
        });
    }

    // ========== python_to_sendable via Python objects ==========

    #[test]
    #[serial]
    fn test_python_to_sendable_primitives() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // None
        let result = crate::rubyx_object::python_to_sendable(api.py_none, api);
        assert!(matches!(result, Ok(SendableValue::Nil)));

        // Integer
        let py_int = api.long_from_i64(42);
        let result = crate::rubyx_object::python_to_sendable(py_int, api);
        assert!(matches!(result, Ok(SendableValue::Integer(42))));
        api.decref(py_int);

        // Float
        let py_float = api.float_from_f64(std::f64::consts::PI);
        let result = crate::rubyx_object::python_to_sendable(py_float, api);
        match result {
            Ok(SendableValue::Float(f)) => assert!((f - std::f64::consts::PI).abs() < 1e-9),
            other => panic!("Expected Float, got {:?}", other),
        }
        api.decref(py_float);

        // String
        let py_str = api.string_from_str("test");
        let result = crate::rubyx_object::python_to_sendable(py_str, api);
        assert!(matches!(result, Ok(SendableValue::Str(s)) if s == "test"));
        api.decref(py_str);

        // Bool true
        let result = crate::rubyx_object::python_to_sendable(api.py_true, api);
        assert!(matches!(result, Ok(SendableValue::Bool(true))));

        // Bool false
        let result = crate::rubyx_object::python_to_sendable(api.py_false, api);
        assert!(matches!(result, Ok(SendableValue::Bool(false))));
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let list = api.list_new(3);
        api.list_set_item(list, 0, api.long_from_i64(10));
        api.list_set_item(list, 1, api.long_from_i64(20));
        api.list_set_item(list, 2, api.long_from_i64(30));

        let result = crate::rubyx_object::python_to_sendable(list, api);
        match result {
            Ok(SendableValue::List(items)) => {
                assert_eq!(items.len(), 3);
                assert!(matches!(items[0], SendableValue::Integer(10)));
                assert!(matches!(items[1], SendableValue::Integer(20)));
                assert!(matches!(items[2], SendableValue::Integer(30)));
            }
            other => panic!("Expected List, got {:?}", other),
        }
        api.decref(list);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_tuple() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let tuple = api.tuple_new(2);
        api.tuple_set_item(tuple, 0, api.string_from_str("a"));
        api.tuple_set_item(tuple, 1, api.long_from_i64(1));

        let result = crate::rubyx_object::python_to_sendable(tuple, api);
        match result {
            Ok(SendableValue::List(items)) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(&items[0], SendableValue::Str(s) if s == "a"));
                assert!(matches!(items[1], SendableValue::Integer(1)));
            }
            other => panic!("Expected List (from tuple), got {:?}", other),
        }
        api.decref(tuple);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_dict() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let dict = api.dict_new();
        let key = api.string_from_str("x");
        let val = api.long_from_i64(99);
        api.dict_set_item(dict, key, val);
        api.decref(key);
        api.decref(val);

        let result = crate::rubyx_object::python_to_sendable(dict, api);
        match result {
            Ok(SendableValue::Dict(entries)) => {
                assert_eq!(entries.len(), 1);
                assert!(matches!(&entries[0].0, SendableValue::Str(s) if s == "x"));
                assert!(matches!(entries[0].1, SendableValue::Integer(99)));
            }
            other => panic!("Expected Dict, got {:?}", other),
        }
        api.decref(dict);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_nested_structure() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Build: {"items": [1, 2], "flag": True}
        let inner_list = api.list_new(2);
        api.list_set_item(inner_list, 0, api.long_from_i64(1));
        api.list_set_item(inner_list, 1, api.long_from_i64(2));

        let dict = api.dict_new();
        let key1 = api.string_from_str("items");
        api.dict_set_item(dict, key1, inner_list);
        api.decref(key1);
        api.decref(inner_list);

        let key2 = api.string_from_str("flag");
        api.incref(api.py_true);
        api.dict_set_item(dict, key2, api.py_true);
        api.decref(key2);

        let result = crate::rubyx_object::python_to_sendable(dict, api);
        match result {
            Ok(SendableValue::Dict(entries)) => {
                assert_eq!(entries.len(), 2);
                // Dict order is not guaranteed; find each key
                let items_entry = entries
                    .iter()
                    .find(|(k, _)| matches!(k, SendableValue::Str(s) if s == "items"))
                    .expect("should have 'items' key");
                assert!(matches!(&items_entry.1, SendableValue::List(l) if l.len() == 2));

                let flag_entry = entries
                    .iter()
                    .find(|(k, _)| matches!(k, SendableValue::Str(s) if s == "flag"))
                    .expect("should have 'flag' key");
                assert!(matches!(flag_entry.1, SendableValue::Bool(true)));
            }
            other => panic!("Expected Dict, got {:?}", other),
        }
        api.decref(dict);
    }

    // ========== RubyxStream::next_item ==========

    #[test]
    #[serial]
    fn test_next_item_returns_values() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Integer(1))).ok();
                tx.send(Some(SendableValue::Integer(2))).ok();
                tx.send(Some(SendableValue::Integer(3))).ok();
                tx.send(None).ok();
            });

            let stream = crate::async_gen::AsyncGeneratorStream::from_channel(rx, cancel_tx);
            let rubyx_stream = crate::rubyx_stream::RubyxStream::from_stream(stream);

            let v1 = rubyx_stream
                .next_item()
                .expect("first next_item should succeed");
            let v2 = rubyx_stream
                .next_item()
                .expect("second next_item should succeed");
            let v3 = rubyx_stream
                .next_item()
                .expect("third next_item should succeed");

            assert_eq!(i64::try_convert(v1).unwrap(), 1);
            assert_eq!(i64::try_convert(v2).unwrap(), 2);
            assert_eq!(i64::try_convert(v3).unwrap(), 3);
        });
    }

    #[test]
    #[serial]
    fn test_next_item_raises_stop_iteration_at_end() {
        with_ruby_python(|ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Integer(42))).ok();
                tx.send(None).ok();
            });

            let stream = crate::async_gen::AsyncGeneratorStream::from_channel(rx, cancel_tx);
            let rubyx_stream = crate::rubyx_stream::RubyxStream::from_stream(stream);

            // First call succeeds
            let val = rubyx_stream.next_item().expect("should return value");
            assert_eq!(i64::try_convert(val).unwrap(), 42);

            // Second call should raise StopIteration
            let err = rubyx_stream
                .next_item()
                .expect_err("should raise StopIteration");
            assert!(
                err.is_kind_of(ruby.exception_stop_iteration()),
                "error should be StopIteration, got: {}",
                err
            );
        });
    }

    #[test]
    #[serial]
    fn test_next_item_empty_stream_raises_stop_iteration() {
        with_ruby_python(|ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(None).ok(); // immediate end
            });

            let stream = crate::async_gen::AsyncGeneratorStream::from_channel(rx, cancel_tx);
            let rubyx_stream = crate::rubyx_stream::RubyxStream::from_stream(stream);

            let err = rubyx_stream
                .next_item()
                .expect_err("should raise StopIteration");
            assert!(
                err.is_kind_of(ruby.exception_stop_iteration()),
                "empty stream should raise StopIteration"
            );
        });
    }

    #[test]
    #[serial]
    fn test_next_item_repeated_after_exhaustion() {
        with_ruby_python(|ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(None).ok();
            });

            let stream = crate::async_gen::AsyncGeneratorStream::from_channel(rx, cancel_tx);
            let rubyx_stream = crate::rubyx_stream::RubyxStream::from_stream(stream);

            // First call raises StopIteration
            let err1 = rubyx_stream
                .next_item()
                .expect_err("should raise StopIteration");
            assert!(err1.is_kind_of(ruby.exception_stop_iteration()));

            // Second call should also raise (StopIteration or stream consumed error)
            let err2 = rubyx_stream
                .next_item()
                .expect_err("should still raise after exhaustion");
            // Could be StopIteration again or RuntimeError("Stream already consumed")
            // Both are acceptable — the key is it doesn't return nil or panic
            assert!(
                err2.is_kind_of(ruby.exception_stop_iteration())
                    || err2.is_kind_of(ruby.exception_runtime_error()),
                "should raise StopIteration or RuntimeError"
            );
        });
    }

    #[test]
    #[serial]
    fn test_next_item_with_mixed_types() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Str("hello".to_string()))).ok();
                tx.send(Some(SendableValue::Float(std::f64::consts::PI)))
                    .ok();
                tx.send(Some(SendableValue::Bool(true))).ok();
                tx.send(Some(SendableValue::Nil)).ok();
                tx.send(None).ok();
            });

            let stream = crate::async_gen::AsyncGeneratorStream::from_channel(rx, cancel_tx);
            let rubyx_stream = crate::rubyx_stream::RubyxStream::from_stream(stream);

            let v1 = rubyx_stream.next_item().unwrap();
            assert_eq!(String::try_convert(v1).unwrap(), "hello");

            let v2 = rubyx_stream.next_item().unwrap();
            assert!((f64::try_convert(v2).unwrap() - std::f64::consts::PI).abs() < 1e-9);

            let v3 = rubyx_stream.next_item().unwrap();
            assert!(bool::try_convert(v3).unwrap());

            let v4 = rubyx_stream.next_item().unwrap();
            assert!(v4.is_nil());
        });
    }

    #[test]
    #[serial]
    fn test_next_item_with_collections() {
        with_ruby_python(|_ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::List(vec![
                    SendableValue::Integer(1),
                    SendableValue::Integer(2),
                ])))
                .ok();
                tx.send(Some(SendableValue::Dict(vec![(
                    SendableValue::Str("k".to_string()),
                    SendableValue::Integer(99),
                )])))
                .ok();
                tx.send(None).ok();
            });

            let stream = crate::async_gen::AsyncGeneratorStream::from_channel(rx, cancel_tx);
            let rubyx_stream = crate::rubyx_stream::RubyxStream::from_stream(stream);

            let arr_val = rubyx_stream.next_item().unwrap();
            let arr = magnus::RArray::try_convert(arr_val).expect("should be Array");
            assert_eq!(arr.len(), 2);

            let hash_val = rubyx_stream.next_item().unwrap();
            let hash = magnus::RHash::try_convert(hash_val).expect("should be Hash");
            let v: i64 = hash.fetch("k").unwrap();
            assert_eq!(v, 99);
        });
    }

    // ========== SendableValue::Bytes → Ruby String (ASCII-8BIT) ==========

    #[test]
    #[serial]
    fn test_sendable_bytes_converts_to_ruby_string() {
        with_ruby_python(|ruby, _api| {
            let val: magnus::Value = SendableValue::Bytes(b"hello".to_vec())
                .try_into()
                .expect("Bytes conversion should succeed");

            // Must be a Ruby String, not an Array of integers
            let rstr = magnus::RString::try_convert(val).expect("should be a String");
            let content = unsafe { rstr.as_slice() };
            assert_eq!(content, b"hello");

            // Verify encoding is ASCII-8BIT
            let enc = rstr.enc_get();
            let ascii_8bit = ruby
                .find_encindex("ASCII-8BIT")
                .expect("ASCII-8BIT must exist");
            assert!(
                enc == ascii_8bit,
                "Bytes should produce ASCII-8BIT encoded String"
            );
        });
    }

    #[test]
    #[serial]
    fn test_sendable_bytes_empty() {
        with_ruby_python(|ruby, _api| {
            let val: magnus::Value = SendableValue::Bytes(Vec::new())
                .try_into()
                .expect("empty Bytes conversion should succeed");

            let rstr = magnus::RString::try_convert(val).expect("should be a String");
            let content = unsafe { rstr.as_slice() };
            assert!(content.is_empty());

            let enc = rstr.enc_get();
            let ascii_8bit = ruby
                .find_encindex("ASCII-8BIT")
                .expect("ASCII-8BIT must exist");
            assert!(enc == ascii_8bit, "encoding should be ASCII-8BIT");
        });
    }

    #[test]
    #[serial]
    fn test_sendable_bytes_with_null_bytes() {
        with_ruby_python(|ruby, _api| {
            let data = vec![0x00, 0x01, 0xff, 0x00, 0xfe];
            let val: magnus::Value = SendableValue::Bytes(data.clone())
                .try_into()
                .expect("Bytes with NULs should convert");

            let rstr = magnus::RString::try_convert(val).expect("should be a String");
            let content = unsafe { rstr.as_slice() };
            assert_eq!(content, data.as_slice());

            let enc = rstr.enc_get();
            let ascii_8bit = ruby
                .find_encindex("ASCII-8BIT")
                .expect("ASCII-8BIT must exist");
            assert!(enc == ascii_8bit, "encoding should be ASCII-8BIT");
        });
    }

    #[test]
    #[serial]
    fn test_sendable_bytes_all_256_values() {
        with_ruby_python(|ruby, _api| {
            let data: Vec<u8> = (0..=255).collect();
            let val: magnus::Value = SendableValue::Bytes(data.clone())
                .try_into()
                .expect("all byte values should convert");

            let rstr = magnus::RString::try_convert(val).expect("should be a String");
            let content = unsafe { rstr.as_slice() };
            assert_eq!(content, data.as_slice());

            let enc = rstr.enc_get();
            let ascii_8bit = ruby
                .find_encindex("ASCII-8BIT")
                .expect("ASCII-8BIT must exist");
            assert!(enc == ascii_8bit, "encoding should be ASCII-8BIT");
        });
    }

    #[test]
    #[serial]
    fn test_sendable_bytes_is_not_array() {
        with_ruby_python(|_ruby, _api| {
            let val: magnus::Value = SendableValue::Bytes(b"test".to_vec())
                .try_into()
                .expect("Bytes conversion should succeed");

            // Must NOT be an Array (Vec<u8>.into_value would produce Array<Integer>)
            assert!(
                magnus::RArray::try_convert(val).is_err(),
                "Bytes must not convert to Ruby Array"
            );
        });
    }

    #[test]
    #[serial]
    fn test_sendable_bytes_in_stream_delivery() {
        with_ruby_python(|ruby, _api| {
            let (tx, rx) = unbounded();
            let (cancel_tx, _cancel_rx) = bounded(1);

            thread::spawn(move || {
                tx.send(Some(SendableValue::Integer(1))).ok();
                tx.send(Some(SendableValue::Bytes(b"\xde\xad".to_vec())))
                    .ok();
                tx.send(Some(SendableValue::Str("after".to_string()))).ok();
                tx.send(None).ok();
            });

            let mut stream = crate::stream::AsyncStream::from_channel(rx, cancel_tx);

            // First: integer
            let val = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(val).unwrap(), 1);

            // Second: bytes → ASCII-8BIT String
            let val = stream.next().unwrap().unwrap();
            let rstr = magnus::RString::try_convert(val).expect("should be String");
            let content = unsafe { rstr.as_slice() };
            assert_eq!(content, b"\xde\xad");
            let enc = rstr.enc_get();
            let ascii_8bit = ruby
                .find_encindex("ASCII-8BIT")
                .expect("ASCII-8BIT must exist");
            assert!(enc == ascii_8bit, "encoding should be ASCII-8BIT");

            // Third: regular string
            let val = stream.next().unwrap().unwrap();
            assert_eq!(String::try_convert(val).unwrap(), "after");

            assert!(stream.next().is_none());
        });
    }

    #[test]
    #[serial]
    fn test_sendable_bytes_large_payload() {
        with_ruby_python(|ruby, _api| {
            let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
            let val: magnus::Value = SendableValue::Bytes(data.clone())
                .try_into()
                .expect("large Bytes should convert");

            let rstr = magnus::RString::try_convert(val).expect("should be a String");
            let content = unsafe { rstr.as_slice() };
            assert_eq!(content.len(), 10_000);
            assert_eq!(content, data.as_slice());

            let enc = rstr.enc_get();
            let ascii_8bit = ruby
                .find_encindex("ASCII-8BIT")
                .expect("ASCII-8BIT must exist");
            assert!(enc == ascii_8bit, "encoding should be ASCII-8BIT");
        });
    }
}
