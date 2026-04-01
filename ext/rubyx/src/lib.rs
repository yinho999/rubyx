use crate::async_gen::{AsyncGeneratorStream, AsyncStrategy};
use crate::nonblocking_stream::NonBlockingStream;
use crate::pipe_notify::PipeNotify;
use crate::python_api::PythonApi;
use crate::rubyx_object::python_to_sendable;
use crate::rubyx_object::RubyxObject;
use crate::rubyx_stream::RubyxStream;
use crate::stream::StreamItem;
use crossbeam_channel::unbounded;
use magnus::typed_data::Obj;
use magnus::{function, method, Module, Object, RArray, Ruby, TryConvert, Value};
use std::sync::Arc;
use std::sync::OnceLock;

mod async_gen;
mod context;
mod convert;
mod eval;
mod exception;
mod future;
mod gvl;
mod import;
mod nonblocking_stream;
mod pipe_notify;
#[allow(dead_code)]
mod python_api;
mod python_ffi;
#[cfg(test)]
mod python_finder;
mod python_guard;
mod ruby_helpers;
mod rubyx_object;
mod rubyx_stream;
mod stream;
#[cfg(test)]
pub mod test_helpers;

// Shared Python API Instance
static API: OnceLock<PythonApi> = OnceLock::new();

/// Global accessor for the Python API.
/// Panics if `Rubyx.init` has not been called yet.
fn api() -> &'static PythonApi {
    API.get().expect("Python not initialized — call Rubyx.init(python_dl, python_home, python_exe, sys_paths) first")
}

#[magnus::init]
fn init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    // Define Rubyx Module
    let rubyx_module = ruby.define_module("Rubyx")?;

    rubyx_module.define_singleton_method("init", function!(rubyx_init, 4))?;
    rubyx_module.define_singleton_method("initialized?", function!(rubyx_initialized, 0))?;

    // Module Class Methods
    rubyx_module.define_singleton_method("_import", function!(crate::import::rubyx_import, 1))?;
    rubyx_module.define_singleton_method("_eval", function!(crate::eval::rubyx_eval, 1))?;
    rubyx_module.define_singleton_method(
        "_eval_with_globals",
        function!(crate::eval::rubyx_eval_with_globals, 2),
    )?;
    rubyx_module.define_singleton_method("_await", function!(crate::eval::rubyx_await, 1))?;
    rubyx_module.define_singleton_method(
        "_await_with_globals",
        function!(crate::eval::rubyx_await_with_globals, 2),
    )?;
    rubyx_module.define_singleton_method("_async_await", function!(rubyx_async_await, 1))?;
    rubyx_module.define_singleton_method(
        "_async_await_with_globals",
        function!(crate::eval::rubyx_async_await_with_globals, 2),
    )?;

    // RubyxObject class for wrapped Python objects
    let py_object = ruby.define_class("RubyxObject", ruby.class_object())?;
    py_object.define_method(
        "method_missing",
        method!(crate::rubyx_object::RubyxObject::method_missing, -1),
    )?;
    py_object.define_method("to_s", method!(crate::rubyx_object::RubyxObject::to_s, 0))?;
    py_object.define_method(
        "inspect",
        method!(crate::rubyx_object::RubyxObject::inspect, 0),
    )?;
    py_object.define_method(
        "to_ruby",
        method!(crate::rubyx_object::RubyxObject::to_ruby, 0),
    )?;
    py_object.define_method("[]", method!(RubyxObject::getitem, 1))?;
    py_object.define_method("[]=", method!(RubyxObject::setitem, 2))?;
    py_object.define_method("delete", method!(RubyxObject::delitem, 1))?;
    py_object.define_method(
        "respond_to_missing?",
        method!(RubyxObject::respond_to_missing, -1),
    )?;
    py_object.define_method("truthy?", method!(RubyxObject::is_truthy, 0))?;
    py_object.define_method("falsy?", method!(RubyxObject::is_falsy, 0))?;
    py_object.define_method("callable?", method!(RubyxObject::is_callable, 0))?;
    py_object.define_method("call", method!(RubyxObject::call, -1))?;
    py_object.define_method("py_type", method!(RubyxObject::py_type, 0))?;
    py_object.define_method("each", method!(RubyxObject::each, 0))?;
    py_object.include_module(ruby.module_enumerable())?;

    // RubyxStream class with Enumerable
    let stream_class = rubyx_module.define_class("Stream", ruby.class_object())?;
    stream_class.define_method("each", method!(crate::rubyx_stream::RubyxStream::each, 0))?;
    stream_class.define_method(
        "next",
        method!(crate::rubyx_stream::RubyxStream::next_item, 0),
    )?;
    stream_class.include_module(ruby.module_enumerable())?;

    // Rubyx.stream(iterable) — creates a RubyxStream from a Python iterable
    rubyx_module.define_singleton_method("stream", function!(create_stream, -1))?;
    // Rubyx.async_stream(iterable) - creates a RubyxStream from rust event loop
    rubyx_module.define_singleton_method("async_stream", function!(create_async_stream, -1))?;

    // NonBlockingStream class with Enumerable
    let nb_stream_class = rubyx_module.define_class("NonBlockingStream", ruby.class_object())?;
    nb_stream_class.define_method(
        "each",
        method!(crate::nonblocking_stream::NonBlockingStream::each, 0),
    )?;
    nb_stream_class.include_module(ruby.module_enumerable())?;

    // Rubyx.nb_stream(iterable) — creates a NonBlockingStream from a Python iterable
    rubyx_module.define_singleton_method("nb_stream", function!(create_nb_stream, 1))?;

    let context_class = rubyx_module.define_class("Context", ruby.class_object())?;
    context_class
        .define_singleton_method("new", function!(crate::context::RubyxContext::new, 0))?;
    context_class.define_method("_eval", method!(crate::context::RubyxContext::eval, 1))?;
    context_class.define_method(
        "_eval_with_globals",
        method!(crate::context::RubyxContext::eval_with_globals, 2),
    )?;
    context_class.define_method(
        "_await",
        method!(crate::context::RubyxContext::await_eval, 1),
    )?;
    context_class.define_method(
        "_await_with_globals",
        method!(crate::context::RubyxContext::await_eval_with_globals, 2),
    )?;
    context_class.define_method(
        "_async_await",
        method!(crate::context::RubyxContext::async_await_eval, 1),
    )?;
    context_class.define_method(
        "_async_await_with_globals",
        method!(
            crate::context::RubyxContext::async_await_eval_with_globals,
            2
        ),
    )?;
    rubyx_module
        .define_singleton_method("context", function!(crate::context::RubyxContext::new, 0))?;

    // Rubyx::Future class
    let future_class = rubyx_module.define_class("Future", ruby.class_object())?;
    // value() instead of await since await is a reserved keyword
    future_class.define_method("await", method!(crate::future::RubyxFuture::value, 0))?;
    future_class.define_method("ready?", method!(crate::future::RubyxFuture::is_ready, 0))?;

    Ok(())
}

/// Rubyx.async_await(coroutine) — runs a Python coroutine on a background thread.
/// Returns a Rubyx::Future immediately. Call future.value to get the result.
fn rubyx_async_await(coroutine: Value) -> Result<future::RubyxFuture, magnus::Error> {
    let obj = Obj::<RubyxObject>::try_convert(coroutine).map_err(|_| {
        magnus::Error::new(
            ruby_helpers::type_error(),
            "Rubyx.async_await requires a Python coroutine (RubyxObject)",
        )
    })?;
    let api = crate::api();
    let gil = api.ensure_gil();

    let future = future::RubyxFuture::from_coroutine(obj.as_ptr(), api);

    api.release_gil(gil);
    Ok(future)
}

fn rubyx_initialized() -> bool {
    API.get().is_some()
}

/// `rubyx_init`: accept config paths and initialize from ruby
fn rubyx_init(
    python_dl: String,
    python_home: String,
    python_exe: String,
    sys_paths: RArray,
) -> Result<bool, magnus::Error> {
    if API.get().is_some() {
        return Err(magnus::Error::new(
            ruby_helpers::runtime_error(),
            "Python Interpreter already initialized",
        ));
    }

    let mut api = unsafe {
        PythonApi::load(std::path::Path::new(&python_dl)).map_err(|e| {
            magnus::Error::new(
                ruby_helpers::runtime_error(),
                format!("Error loading Python interpreter: {e}"),
            )
        })?
    };

    api.set_python_home(&python_home);
    api.set_program_name(&python_exe);

    api.initialize_ex(0);

    if !api.is_initialized() {
        return Err(magnus::Error::new(
            ruby_helpers::runtime_error(),
            "Python Interpreter failed to initialize",
        ));
    }

    inject_sys_paths(&api, &sys_paths)?;

    let _ = api.install_async_to_sync_class();

    // Release the GIL that Py_InitializeEx() acquired on the main thread.
    // This allows worker threads to acquire it via ensure_gil()/release_gil().
    // Without this, the main thread permanently holds the GIL and any
    // background thread calling ensure_gil() will deadlock.
    api.save_thread();

    API.set(api).map_err(|_| {
        magnus::Error::new(ruby_helpers::runtime_error(), "Failed to store Python API")
    })?;

    Ok(true)
}

fn inject_sys_paths(api: &PythonApi, sys_paths: &RArray) -> Result<(), magnus::Error> {
    let sys_module = api.import_module("sys").map_err(|e| {
        magnus::Error::new(
            ruby_helpers::runtime_error(),
            format!("Failed to import sys module: {e}"),
        )
    })?;
    let sys = api.object_get_attr_string(sys_module, "path");
    if sys.is_null() {
        api.decref(sys_module);
        return Err(magnus::Error::new(
            ruby_helpers::runtime_error(),
            "Failed to get sys.path",
        ));
    }

    // Append sys_paths to sys.path
    let len = sys_paths.len();
    for i in 0..len {
        let path: String = sys_paths.entry(i as isize).map_err(|e| {
            magnus::Error::new(
                ruby_helpers::runtime_error(),
                format!("Failed to get path at index {i}: {e}"),
            )
        })?;
        let py_str = api.string_from_str(&path);
        if py_str.is_null() {
            continue;
        }
        let result = api.list_append(sys, py_str);
        if result == -1 {
            api.decref(py_str);
            continue;
        }
        api.decref(py_str);
    }
    api.decref(sys);
    api.decref(sys_module);

    api.clear_error();
    Ok(())
}

fn create_stream(args: &[Value]) -> Result<rubyx_stream::RubyxStream, magnus::Error> {
    let ruby = Ruby::get().map_err(|e| {
        magnus::Error::new(
            ruby_helpers::runtime_error(),
            format!("Error getting Ruby: {e}"),
        )
    })?;
    let has_block = ruby.block_given();
    if args.len() == 1 && !has_block {
        create_stream_from_iterable(args[0])
    } else if args.is_empty() && has_block {
        // Get proc
        let proc = ruby.block_proc().map_err(|e| {
            magnus::Error::new(
                ruby_helpers::runtime_error(),
                format!("Error getting block proc: {e}"),
            )
        })?;
        // Run Proc
        let iterable: Value = proc.call(())?;
        create_stream_from_iterable(iterable)
    } else {
        Err(magnus::Error::new(
            ruby_helpers::arg_error(),
            "Rubyx.stream takes either 0 or 1 arguments",
        ))
    }
}

fn create_nb_stream(
    iterable: Value,
) -> Result<nonblocking_stream::NonBlockingStream, magnus::Error> {
    let obj = Obj::<RubyxObject>::try_convert(iterable).map_err(|_| {
        magnus::Error::new(
            ruby_helpers::type_error(),
            "Rubyx.nb_stream requires a Python object (RubyxObject)",
        )
    })?;

    let api = crate::api();
    let gil = api.ensure_gil();

    // Get a Python iterator — handle both sync and async iterables
    let py_iter = if api.is_async_iterable(obj.as_ptr()) {
        let sync_iter = api.wrap_async_generator(obj.as_ptr());
        if sync_iter.is_null() {
            api.clear_error();
            api.release_gil(gil);
            return Err(magnus::Error::new(
                ruby_helpers::runtime_error(),
                "Failed to wrap async generator",
            ));
        }
        sync_iter
    } else {
        let iter = api.object_get_iter(obj.as_ptr());
        if iter.is_null() {
            api.clear_error();
            api.release_gil(gil);
            return Err(magnus::Error::new(
                ruby_helpers::type_error(),
                "Object is not iterable",
            ));
        }
        iter
    };

    api.release_gil(gil);

    // Use unbounded channel so the producer never blocks on send().
    // With a bounded channel, the producer blocks when the channel is
    // full. If the consumer is in IO.select (fiber-aware path) waiting
    // for a pipe notification, and the producer can't reach notify()
    // because it's blocked on send(), both sides deadlock.
    let (tx, rx) = unbounded();
    let pipe = Arc::new(PipeNotify::new().map_err(|e| {
        magnus::Error::new(
            ruby_helpers::runtime_error(),
            format!("Failed to create pipe: {e}"),
        )
    })?);
    let pipe_clone = pipe.clone();
    let py_iter_addr = py_iter as usize;

    std::thread::spawn(move || {
        let py_iter = py_iter_addr as *mut crate::python_ffi::PyObject;
        let api = crate::api();
        let gil = api.ensure_gil();

        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                if api.has_error() {
                    if let Some(exc) = crate::python_api::PythonApi::extract_exception(api) {
                        let _ = tx.send(StreamItem::Error(exc.to_string()));
                    } else {
                        let _ = tx.send(StreamItem::End);
                    }
                } else {
                    let _ = tx.send(StreamItem::End);
                }
                pipe_clone.notify();
                break;
            }

            let ruby_value = python_to_sendable(item, api)
                .map_err(|e| format!("Error converting Python value: {e}"));
            api.decref(item);

            match ruby_value {
                Ok(value) => {
                    if tx.send(StreamItem::Value(value)).is_err() {
                        break; // Consumer dropped
                    }
                    pipe_clone.notify();
                }
                Err(e) => {
                    let _ = tx.send(StreamItem::Error(e));
                    pipe_clone.notify();
                    break;
                }
            }
        }

        api.decref(py_iter);
        api.release_gil(gil);
    });

    Ok(NonBlockingStream::new(rx, pipe))
}

fn create_async_stream(args: &[Value]) -> Result<rubyx_stream::RubyxStream, magnus::Error> {
    let ruby = Ruby::get().map_err(|e| {
        magnus::Error::new(
            ruby_helpers::runtime_error(),
            format!("Error getting Ruby: {e}"),
        )
    })?;
    let has_block = ruby.block_given();
    if args.len() == 1 && !has_block {
        create_async_stream_from_iterable(args[0])
    } else if args.is_empty() && has_block {
        // Get proc
        let proc = ruby.block_proc().map_err(|e| {
            magnus::Error::new(
                ruby_helpers::runtime_error(),
                format!("Error getting block proc: {e}"),
            )
        })?;
        // Run Proc
        let iterable: Value = proc.call(())?;
        create_async_stream_from_iterable(iterable)
    } else {
        Err(magnus::Error::new(
            ruby_helpers::arg_error(),
            "Rubyx.stream takes either 0 or 1 arguments",
        ))
    }
}

/// Create a RubyxStream from a Python iterable (RubyxObject).
///
/// Acquires the GIL, calls PyObject_GetIter on the wrapped Python object,
/// and passes the resulting iterator to AsyncStream::from_python_iterator.
fn create_stream_from_iterable(iterable: Value) -> Result<RubyxStream, magnus::Error> {
    let obj = Obj::<RubyxObject>::try_convert(iterable).map_err(|_| {
        magnus::Error::new(
            ruby_helpers::type_error(),
            "Rubyx.stream requires a Python object (RubyxObject)",
        )
    })?;

    let stream =
        AsyncGeneratorStream::from_python_object(obj.as_ptr(), AsyncStrategy::PythonAdapter)
            .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e))?;

    Ok(RubyxStream::from_stream(stream))
}

fn create_async_stream_from_iterable(iterable: Value) -> Result<RubyxStream, magnus::Error> {
    let obj = Obj::<RubyxObject>::try_convert(iterable).map_err(|_| {
        magnus::Error::new(
            ruby_helpers::type_error(),
            "Rubyx.stream requires a Python object (RubyxObject)",
        )
    })?;

    // Verify the object is actually an async iterable before using RustDriving
    let api = crate::api();
    let gil = api.ensure_gil();
    let is_async = api.is_async_iterable(obj.as_ptr());
    api.release_gil(gil);

    if !is_async {
        return Err(magnus::Error::new(
            ruby_helpers::type_error(),
            "Object is not an async iterable (missing __aiter__/__anext__)",
        ));
    }

    let stream = AsyncGeneratorStream::from_python_object(obj.as_ptr(), AsyncStrategy::RustDriving)
        .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e))?;

    Ok(RubyxStream::from_stream(stream))
}
// export LD_LIBRARY_PATH="~/.asdf/installs/ruby/3.4.7/lib:$LD_LIBRARY_PATH"
// cargo test

#[cfg(test)]
mod tests {
    use crate::rubyx_object::RubyxObject;
    use crate::test_helpers::{skip_if_no_python, with_ruby_python};
    use magnus::typed_data::Obj;
    use magnus::value::ReprValue;
    use magnus::{IntoValue, TryConvert};
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_rubyx_object_wraps_pyobject() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        assert!(!py_int.is_null());

        let wrapper = RubyxObject::new(py_int, api).expect("Should wrap non-null PyObject");
        assert_eq!(
            wrapper.as_ptr(),
            py_int,
            "as_ptr should return the original pointer"
        );

        // Drop wrapper (decrefs: refcount 2 → 1), then decref original (1 → 0)
        drop(wrapper);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_rubyx_object_null_returns_none() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let result = RubyxObject::new(std::ptr::null_mut(), api);
        assert!(result.is_none(), "null pointer should return None");
    }

    #[test]
    #[serial]
    fn test_rubyx_object_increfs_on_create() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // long_from_i64 returns a new reference (refcount = 1)
        let py_int = api.long_from_i64(42);

        // Wrapping increfs (refcount = 2)
        let wrapper = RubyxObject::new(py_int, api).unwrap();

        // Release our original reference (refcount = 1, wrapper still holds it)
        api.decref(py_int);

        // Object must still be alive — the wrapper's incref keeps it alive
        let value = api.long_to_i64(wrapper.as_ptr());
        assert_eq!(
            value, 42,
            "Object should still be alive after decref'ing original ref"
        );

        // wrapper drops here → decref → refcount 0 → freed
    }

    #[test]
    #[serial]
    fn test_rubyx_object_decrefs_on_drop() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Create object (refcount = 1)
        let py_int = api.long_from_i64(99);

        // Take an extra ref so we can safely observe after wrapper drops (refcount = 2)
        api.incref(py_int);

        {
            // Wrap it (incref → refcount = 3)
            let wrapper = RubyxObject::new(py_int, api).unwrap();
            assert_eq!(api.long_to_i64(wrapper.as_ptr()), 99);
            // wrapper drops here → decref → refcount = 2
        }

        // Object should still be alive (refcount = 2)
        let value = api.long_to_i64(py_int);
        assert_eq!(value, 99, "Object should survive after wrapper drop");

        // Clean up our two remaining references
        api.decref(py_int);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_multiple_wrappers_same_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // refcount = 1
        let py_int = api.long_from_i64(77);

        // Each wrapper increfs: refcount 1 → 2 → 3 → 4
        let w1 = RubyxObject::new(py_int, api).unwrap();
        let w2 = RubyxObject::new(py_int, api).unwrap();
        let w3 = RubyxObject::new(py_int, api).unwrap();

        // All point to the same object
        assert_eq!(w1.as_ptr(), py_int);
        assert_eq!(w2.as_ptr(), py_int);
        assert_eq!(w3.as_ptr(), py_int);

        // Drop wrappers: each decrefs (refcount 4 → 3 → 2 → 1)
        drop(w1);
        drop(w2);
        drop(w3);

        // Object still alive (refcount = 1, our original ref)
        assert_eq!(api.long_to_i64(py_int), 77);

        // Clean up
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_gc_stress() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Rapid create-and-drop: 1000 wrappers sequentially
        for i in 0..1000 {
            let py_int = api.long_from_i64(i);
            let wrapper = RubyxObject::new(py_int, api).unwrap();

            assert_eq!(api.long_to_i64(wrapper.as_ptr()), i);

            drop(wrapper);
            api.decref(py_int);
        }

        // Many wrappers alive simultaneously
        let mut wrappers = Vec::new();
        for i in 0..100 {
            let py_int = api.long_from_i64(i);
            wrappers.push((py_int, RubyxObject::new(py_int, api).unwrap()));
        }

        // Verify all still valid while all alive
        for (i, (_ptr, wrapper)) in wrappers.iter().enumerate() {
            assert_eq!(api.long_to_i64(wrapper.as_ptr()), i as i64);
        }

        // Drop all wrappers and clean up
        for (ptr, wrapper) in wrappers {
            drop(wrapper);
            api.decref(ptr);
        }
    }

    // ========== Multi-threading safety tests ==========
    //
    // These tests validate the `unsafe impl Send` and `unsafe impl Sync`
    // on RubyxObject. They exercise cross-thread GIL acquisition, concurrent
    // Drop, and shared-reference access from multiple threads.
    //
    // Python's GIL serialises interpreter access, so "thread safety" here means:
    //   1. No crashes / UB when operations happen from different OS threads.
    //   2. Reference counts remain consistent after concurrent create/drop.
    //   3. ensure_gil / release_gil work correctly from non-main threads.

    /// SAFETY: the wrapped pointer is only dereferenced while holding
    /// the Python GIL, which serialises all interpreter access.
    struct SendPtr(*mut crate::python_ffi::PyObject);
    unsafe impl Send for SendPtr {}
    unsafe impl Sync for SendPtr {}

    fn get_static_api() -> Option<&'static crate::python_api::PythonApi> {
        crate::test_helpers::get_api()
    }

    #[test]
    #[serial]
    fn test_send_wrapper_to_another_thread() {
        use std::thread;

        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        api.incref(py_int);
        let wrapper = RubyxObject::new(py_int, api).unwrap();

        let saved = api.save_thread();
        let static_api = get_static_api().unwrap();

        let handle = thread::spawn(move || {
            let gil = static_api.ensure_gil();
            let val = static_api.long_to_i64(wrapper.as_ptr());
            assert_eq!(val, 42, "Value should survive cross-thread move");
            drop(wrapper);
            static_api.release_gil(gil);
        });

        handle.join().expect("Worker thread panicked");
        api.restore_thread(saved);

        let val = api.long_to_i64(py_int);
        assert_eq!(
            val, 42,
            "Object should survive after cross-thread wrapper drop"
        );
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_concurrent_create_and_drop_different_objects() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let num_threads = 4;
        let objects_per_thread = 100;

        let saved = api.save_thread();
        let static_api = get_static_api().unwrap();

        let barrier = Arc::new(Barrier::new(num_threads));
        let mut handles = Vec::new();

        for t in 0..num_threads {
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();

                for i in 0..objects_per_thread {
                    let gil = static_api.ensure_gil();
                    let value = (t * objects_per_thread + i) as i64;
                    let py_int = static_api.long_from_i64(value);
                    let wrapper = RubyxObject::new(py_int, static_api).unwrap();
                    assert_eq!(static_api.long_to_i64(wrapper.as_ptr()), value);
                    drop(wrapper);
                    static_api.decref(py_int);
                    static_api.release_gil(gil);
                }
            }));
        }

        for h in handles {
            h.join().expect("Worker thread panicked");
        }

        api.restore_thread(saved);
    }

    #[test]
    #[serial]
    fn test_concurrent_wrappers_same_object() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(999);
        api.incref(py_int);

        let num_threads = 4;
        let wraps_per_thread = 50;
        let shared_ptr = Arc::new(SendPtr(py_int));

        let saved = api.save_thread();
        let static_api = get_static_api().unwrap();

        let barrier = Arc::new(Barrier::new(num_threads));
        let mut handles = Vec::new();

        for _ in 0..num_threads {
            let barrier = Arc::clone(&barrier);
            let shared_ptr = Arc::clone(&shared_ptr);
            handles.push(thread::spawn(move || {
                barrier.wait();

                for _ in 0..wraps_per_thread {
                    let gil = static_api.ensure_gil();
                    let wrapper = RubyxObject::new(shared_ptr.0, static_api).unwrap();
                    assert_eq!(static_api.long_to_i64(wrapper.as_ptr()), 999);
                    drop(wrapper);
                    static_api.release_gil(gil);
                }
            }));
        }

        for h in handles {
            h.join().expect("Worker thread panicked");
        }

        api.restore_thread(saved);

        let val = api.long_to_i64(py_int);
        assert_eq!(
            val, 999,
            "Object should survive concurrent wrap/drop cycles"
        );

        api.decref(py_int);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_drop_on_different_thread_than_create() {
        use std::thread;

        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_ints: Vec<_> = (0..10)
            .map(|i| {
                let p = api.long_from_i64(i);
                api.incref(p);
                let wrapper = RubyxObject::new(p, api).unwrap();
                (SendPtr(p), wrapper)
            })
            .collect();

        let saved = api.save_thread();
        let static_api = get_static_api().unwrap();

        let mut handles = Vec::new();
        for (send_ptr, wrapper) in py_ints.into_iter() {
            handles.push(thread::spawn(move || {
                let gil = static_api.ensure_gil();
                drop(wrapper);
                static_api.release_gil(gil);
                send_ptr
            }));
        }

        let returned_ptrs: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("Worker panicked"))
            .collect();

        let gil = static_api.ensure_gil();
        for (i, send_ptr) in returned_ptrs.iter().enumerate() {
            let val = static_api.long_to_i64(send_ptr.0);
            assert_eq!(val, i as i64, "Object {i} should survive cross-thread drop");
            static_api.decref(send_ptr.0);
        }
        static_api.release_gil(gil);

        api.restore_thread(saved);
    }

    #[test]
    #[serial]
    fn test_shared_ref_across_threads() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(777);
        // Arc<RubyxObject> exercises Sync — multiple threads hold &RubyxObject
        let wrapper = Arc::new(RubyxObject::new(py_int, api).unwrap());

        let num_threads = 4;
        let reads_per_thread = 100;

        let saved = api.save_thread();
        let static_api = get_static_api().unwrap();

        let barrier = Arc::new(Barrier::new(num_threads));
        let mut handles = Vec::new();

        for _ in 0..num_threads {
            let wrapper = Arc::clone(&wrapper);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                for _ in 0..reads_per_thread {
                    let gil = static_api.ensure_gil();
                    let val = static_api.long_to_i64(wrapper.as_ptr());
                    assert_eq!(val, 777, "Concurrent reads should be consistent");
                    static_api.release_gil(gil);
                }
            }));
        }

        for h in handles {
            h.join().expect("Worker panicked");
        }

        api.restore_thread(saved);

        drop(wrapper);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_concurrent_stress_mixed_operations() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let num_threads = 8;
        let iterations = 50;

        let saved = api.save_thread();
        let static_api = get_static_api().unwrap();

        let barrier = Arc::new(Barrier::new(num_threads));
        let mut handles = Vec::new();

        for t in 0..num_threads {
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();

                let mut wrappers = Vec::new();

                for i in 0..iterations {
                    let gil = static_api.ensure_gil();

                    let val = (t * 1000 + i) as i64;
                    let py_int = static_api.long_from_i64(val);
                    let w = RubyxObject::new(py_int, static_api).unwrap();
                    assert_eq!(static_api.long_to_i64(w.as_ptr()), val);
                    wrappers.push((py_int, w));

                    if wrappers.len() > 5 {
                        let (ptr, w) = wrappers.remove(0);
                        drop(w);
                        static_api.decref(ptr);
                    }

                    static_api.release_gil(gil);
                }

                let gil = static_api.ensure_gil();
                for (ptr, w) in wrappers {
                    drop(w);
                    static_api.decref(ptr);
                }
                static_api.release_gil(gil);
            }));
        }

        for h in handles {
            h.join().expect("Worker panicked");
        }

        api.restore_thread(saved);
    }

    #[test]
    #[serial]
    fn test_gil_ensure_is_reentrant() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // RubyxObject::new calls ensure_gil internally.
        // The guard already holds the GIL, so this exercises reentrant acquire.
        let py_int = api.long_from_i64(123);
        let wrapper = RubyxObject::new(py_int, api).unwrap();
        assert_eq!(api.long_to_i64(wrapper.as_ptr()), 123);

        let inner_gil = api.ensure_gil();
        let py_int2 = api.long_from_i64(456);
        let wrapper2 = RubyxObject::new(py_int2, api).unwrap();
        assert_eq!(api.long_to_i64(wrapper2.as_ptr()), 456);
        drop(wrapper2);
        api.decref(py_int2);
        api.release_gil(inner_gil);

        drop(wrapper);
        api.decref(py_int);
    }

    // ========== Import tests ==========

    #[test]
    #[serial]
    fn test_import_builtin_module() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Import 'sys' — a builtin module that's always available
        let module = api.import_module("sys");
        assert!(module.is_ok(), "Should import 'sys' module");
        let module = module.unwrap();
        assert!(!module.is_null(), "sys module should be non-null");

        // Wrap in RubyxObject to verify the full pipeline
        let wrapper = RubyxObject::new(module, api);
        assert!(wrapper.is_some(), "Should wrap imported module");
        let wrapper = wrapper.unwrap();
        assert_eq!(wrapper.as_ptr(), module);

        drop(wrapper);
        api.decref(module);
    }

    #[test]
    #[serial]
    fn test_import_nonexistent_raises() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Importing a non-existent module should fail
        let result = api.import_module("nonexistent_module_xyz_12345");
        assert!(result.is_err(), "Importing nonexistent module should fail");

        // Python should have set an error (ModuleNotFoundError / ImportError)
        // import_module returns Err on null, but the error state may or may not
        // still be set depending on implementation. Clear to be safe.
        if api.has_error() {
            let exc = crate::python_api::PythonApi::extract_exception(api);
            assert!(exc.is_some(), "Should have a Python exception");
            if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
                // Python 3.6+ raises ModuleNotFoundError (subclass of ImportError)
                assert!(
                    kind == "ModuleNotFoundError" || kind == "ImportError",
                    "Expected ModuleNotFoundError or ImportError, got: {}",
                    kind
                );
            }
        }
    }

    #[test]
    #[serial]
    fn test_import_json_module_and_use() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // json is a pure-Python module — always safe to import via libloading
        let module = api.import_module("json").expect("json should import");
        assert!(!module.is_null());

        // Wrap in RubyxObject like rubyx_import does
        let wrapper = RubyxObject::new(module, api).expect("Should wrap json module");
        assert!(!wrapper.as_ptr().is_null());

        drop(wrapper);
        api.decref(module);
    }

    #[test]
    #[serial]
    fn test_import_os_module() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let module = api.import_module("os").expect("os should import");
        assert!(!module.is_null());

        let wrapper = RubyxObject::new(module, api).expect("Should wrap os module");
        drop(wrapper);
        api.decref(module);
    }

    #[test]
    #[serial]
    fn test_import_same_module_twice() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let module1 = api
            .import_module("sys")
            .expect("First sys import should work");
        let module2 = api
            .import_module("sys")
            .expect("Second sys import should work");

        // Python caches modules — both should be the same object
        assert_eq!(
            module1, module2,
            "Importing the same module twice should return the same object"
        );

        api.decref(module1);
        api.decref(module2);
    }

    // ========== Eval tests ==========
    //
    // Same constraint as import: rubyx_eval() returns magnus::Value and uses
    // crate::api(), so we test the underlying operations directly.
    // We replicate what eval.rs does: make_globals → run_string → wrap result.

    /// Py_eval_input = 258 (for expressions)
    const EVAL_INPUT: i64 = 258;
    /// Py_file_input = 257 (for statements)
    const FILE_INPUT: i64 = 257;

    /// Helper: create globals dict with __builtins__ (mirrors eval::make_globals)
    fn test_make_globals(api: &crate::python_api::PythonApi) -> *mut crate::python_ffi::PyObject {
        let globals = api.dict_new();
        let builtins_key = api.string_from_str("__builtins__");
        let builtins = api
            .import_module("builtins")
            .expect("builtins should exist");
        api.dict_set_item(globals, builtins_key, builtins);
        api.decref(builtins_key);
        api.decref(builtins);
        globals
    }

    #[test]
    #[serial]
    fn test_eval_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        // Evaluate "1 + 2" as expression
        let result = api.run_string("1 + 2", EVAL_INPUT, globals, globals);
        assert!(result.is_ok(), "run_string should succeed");
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null(), "Expression result should be non-null");
        assert_eq!(api.long_to_i64(py_obj), 3, "1 + 2 should equal 3");

        // Wrap in RubyxObject like rubyx_eval does
        let wrapper = RubyxObject::new(py_obj, api).expect("Should wrap eval result");
        assert_eq!(api.long_to_i64(wrapper.as_ptr()), 3);

        drop(wrapper);
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_string_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("'hello' + ' ' + 'world'", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert_eq!(
            api.string_to_string(py_obj),
            Some("hello world".to_string())
        );

        let wrapper = RubyxObject::new(py_obj, api).expect("Should wrap string result");
        assert_eq!(
            api.string_to_string(wrapper.as_ptr()),
            Some("hello world".to_string())
        );

        drop(wrapper);
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_with_syntax_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        // "def" alone is invalid as an expression (Py_eval_input)
        let result = api.run_string("def", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null(), "Invalid expression should return null");
        assert!(api.has_error(), "Python error should be set");

        // Verify it's a SyntaxError
        let exc = crate::python_api::PythonApi::extract_exception(api);
        assert!(exc.is_some(), "Should have extracted an exception");
        assert!(
            matches!(
                exc,
                Some(crate::exception::PythonException::SyntaxError { .. })
            ),
            "Expected SyntaxError, got: {:?}",
            exc
        );

        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_syntax_error_then_retry_as_statement() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        // "x = 42" is invalid as expression but valid as statement
        let result = api.run_string("x = 42", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(
            py_obj.is_null(),
            "Assignment should fail as expression (eval_input)"
        );

        // Check it's a syntax error
        let is_syntax = api.has_error() && {
            let exc = crate::python_api::PythonApi::extract_exception(api);
            matches!(
                exc,
                Some(crate::exception::PythonException::SyntaxError { .. })
            )
        };
        assert!(
            is_syntax,
            "Assignment should produce SyntaxError as expression"
        );

        // Retry as statement (Py_file_input) — this is what rubyx_eval does
        let result = api.run_string("x = 42", FILE_INPUT, globals, globals);
        let stmt_obj = result.unwrap();
        assert!(
            !stmt_obj.is_null(),
            "Assignment should succeed as statement"
        );
        assert!(api.is_none(stmt_obj), "Statement should return Py_None");

        // Verify the variable was set
        let key = api.string_from_str("x");
        let val = api.dict_get_item(globals, key);
        assert!(!val.is_null(), "x should exist in globals");
        assert_eq!(api.long_to_i64(val), 42, "x should be 42");

        api.decref(key);
        api.decref(stmt_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_name_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("undefined_variable_xyz", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null(), "Undefined variable should return null");
        assert!(api.has_error(), "NameError should be set");

        let exc = crate::python_api::PythonApi::extract_exception(api);
        assert!(exc.is_some());
        if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
            assert_eq!(kind, "NameError", "Should be a NameError");
        } else {
            panic!("Expected Exception variant with NameError, got: {:?}", exc);
        }

        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_division_by_zero() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("1 / 0", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null(), "Division by zero should return null");
        assert!(api.has_error());

        let exc = crate::python_api::PythonApi::extract_exception(api);
        if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
            assert_eq!(kind, "ZeroDivisionError");
        } else {
            panic!("Expected ZeroDivisionError, got: {:?}", exc);
        }

        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_builtin_function() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("len([1, 2, 3, 4, 5])", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert_eq!(api.long_to_i64(py_obj), 5, "len([1,2,3,4,5]) should be 5");

        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_list_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("[x * 2 for x in range(5)]", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert!(api.list_check(py_obj), "Should return a list");
        assert_eq!(api.list_size(py_obj), 5, "List should have 5 elements");

        // Check values: [0, 2, 4, 6, 8]
        assert_eq!(api.long_to_i64(api.list_get_item(py_obj, 0)), 0);
        assert_eq!(api.long_to_i64(api.list_get_item(py_obj, 2)), 4);
        assert_eq!(api.long_to_i64(api.list_get_item(py_obj, 4)), 8);

        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_dict_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("{'a': 1, 'b': 2}", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert!(api.dict_check(py_obj), "Should return a dict");
        assert_eq!(api.dict_size(py_obj), 2);

        let key_a = api.string_from_str("a");
        let val_a = api.dict_get_item(py_obj, key_a);
        assert_eq!(api.long_to_i64(val_a), 1);
        api.decref(key_a);

        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_none_result() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        // print() returns None
        let result = api.run_string("print('hello from test')", FILE_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert!(api.is_none(py_obj), "Statement result should be Py_None");

        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_bool_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("10 > 5", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert!(api.is_true(py_obj), "10 > 5 should be True");

        let result2 = api.run_string("10 < 5", EVAL_INPUT, globals, globals);
        let py_obj2 = result2.unwrap();
        assert!(!py_obj2.is_null());
        assert!(api.is_false(py_obj2), "10 < 5 should be False");

        api.decref(py_obj);
        api.decref(py_obj2);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_float_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("3.14 * 2", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert!(api.is_float(py_obj), "Should return a float");
        let value = api.float_to_f64(py_obj);
        assert!(
            (value - 6.28).abs() < 0.001,
            "3.14 * 2 should be ~6.28, got {}",
            value
        );

        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_multiline_statement() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let code = "def factorial(n):\n    return 1 if n <= 1 else n * factorial(n - 1)\nresult = factorial(5)";
        let result = api.run_string(code, FILE_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null(), "Multiline statement should succeed");

        let key = api.string_from_str("result");
        let val = api.dict_get_item(globals, key);
        assert!(!val.is_null(), "result should exist in globals");
        assert_eq!(api.long_to_i64(val), 120, "factorial(5) should be 120");

        api.decref(key);
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_with_import_in_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        // First set up the import as a statement
        let setup = api.run_string("import json", FILE_INPUT, globals, globals);
        assert!(!setup.unwrap().is_null());

        // Then evaluate using the imported module
        let result = api.run_string(
            "json.loads('{\"key\": 42}')['key']",
            EVAL_INPUT,
            globals,
            globals,
        );
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert_eq!(api.long_to_i64(py_obj), 42);

        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_result_wraps_in_rubyx_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("42 * 10", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());

        // This is what rubyx_eval does: wrap in RubyxObject
        let wrapper = RubyxObject::new(py_obj, api).expect("Should wrap result");
        assert_eq!(api.long_to_i64(wrapper.as_ptr()), 420);

        // After wrapping, the original ref can be decref'd safely
        api.decref(py_obj);

        // Wrapper still holds a valid reference
        assert_eq!(api.long_to_i64(wrapper.as_ptr()), 420);

        drop(wrapper);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_eval_error_does_not_leak_globals() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Run multiple eval-like cycles with errors — should not leak
        for _ in 0..100 {
            let globals = test_make_globals(api);
            let result = api.run_string("undefined_var", EVAL_INPUT, globals, globals);
            let py_obj = result.unwrap();
            assert!(py_obj.is_null());
            api.clear_error();
            api.decref(globals);
        }

        // If we get here without crash, no memory corruption from repeated cycles
    }

    #[test]
    #[serial]
    fn test_eval_sequential_expressions_share_globals() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        // Set a variable via statement
        let r1 = api.run_string("x = 10", FILE_INPUT, globals, globals);
        assert!(!r1.unwrap().is_null());

        // Use it in a subsequent expression
        let r2 = api.run_string("x * 5", EVAL_INPUT, globals, globals);
        let py_obj = r2.unwrap();
        assert!(!py_obj.is_null());
        assert_eq!(api.long_to_i64(py_obj), 50, "x * 5 should be 50");

        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_method_call() {
        use magnus::typed_data::Obj;
        use magnus::{IntoValue, TryConvert};

        with_ruby_python(|ruby, api| {
            let json = api.import_module("json").expect("json should import");
            let wrapper = RubyxObject::new(json, api).expect("wrapper should be created");

            let args = vec![
                "loads".into_value_with(ruby),
                r#"{"x": 42}"#.into_value_with(ruby),
            ];
            let result = wrapper
                .method_missing(&args)
                .expect("loads call should succeed");
            let py_result = Obj::<RubyxObject>::try_convert(result)
                .expect("result should be wrapped Python object");
            assert!(
                api.dict_check(py_result.as_ptr()),
                "json.loads should return a dict"
            );

            let key = api.string_from_str("x");
            let val = api.dict_get_item(py_result.as_ptr(), key);
            assert_eq!(api.long_to_i64(val), 42);
            api.decref(key);

            drop(wrapper);
            api.decref(json);
        });
    }

    #[test]
    #[serial]
    fn test_attribute_get() {
        use magnus::typed_data::Obj;
        use magnus::{IntoValue, TryConvert};

        with_ruby_python(|ruby, api| {
            let sys = api.import_module("sys").expect("sys should import");
            let wrapper = RubyxObject::new(sys, api).expect("wrapper should be created");

            let args = vec!["version".into_value_with(ruby)];
            let result = wrapper
                .method_missing(&args)
                .expect("attribute read should succeed");
            let py_result = Obj::<RubyxObject>::try_convert(result)
                .expect("result should be wrapped Python object");
            assert!(
                api.is_string(py_result.as_ptr()),
                "sys.version should be a string"
            );
            let version = api
                .string_to_string(py_result.as_ptr())
                .expect("version should decode as string");
            assert!(!version.is_empty(), "version string should not be empty");

            drop(wrapper);
            api.decref(sys);
        });
    }

    #[test]
    #[serial]
    fn test_attribute_set() {
        use magnus::typed_data::Obj;
        use magnus::{IntoValue, TryConvert};

        with_ruby_python(|ruby, api| {
            let types = api.import_module("types").expect("types should import");
            let wrapper = RubyxObject::new(types, api).expect("wrapper should be created");

            // ns = types.SimpleNamespace()
            let args = vec!["SimpleNamespace".into_value_with(ruby)];
            let ns_result = wrapper
                .method_missing(&args)
                .expect("SimpleNamespace() should succeed");
            let ns = Obj::<RubyxObject>::try_convert(ns_result)
                .expect("result should be wrapped Python object");

            // ns.foo = 99
            let ns_obj = RubyxObject::new(ns.as_ptr(), api).expect("rewrap should succeed");
            let set_args = vec!["foo=".into_value_with(ruby), 99_i64.into_value_with(ruby)];
            ns_obj
                .method_missing(&set_args)
                .expect("setter should succeed");

            // ns.foo → 99
            let get_args = vec!["foo".into_value_with(ruby)];
            let get_result = ns_obj
                .method_missing(&get_args)
                .expect("getter should succeed");
            let py_val = Obj::<RubyxObject>::try_convert(get_result)
                .expect("result should be wrapped Python object");
            assert_eq!(api.long_to_i64(py_val.as_ptr()), 99);

            drop(ns_obj);
            drop(wrapper);
            api.decref(types);
        });
    }

    #[test]
    #[serial]
    fn test_keyword_arguments() {
        use magnus::typed_data::Obj;
        use magnus::{IntoValue, TryConvert};

        with_ruby_python(|ruby, api| {
            let json = api.import_module("json").expect("json should import");
            let wrapper = RubyxObject::new(json, api).expect("wrapper should be created");

            // Build a Python dict to serialize: {"a": 1}
            let py_dict = api.dict_new();
            let py_key = api.string_from_str("a");
            let py_val = api.long_from_i64(1);
            api.dict_set_item(py_dict, py_key, py_val);
            api.decref(py_key);
            api.decref(py_val);
            let dict_wrapper =
                RubyxObject::new(py_dict, api).expect("dict wrapper should be created");
            let dict_value = magnus::IntoValue::into_value_with(dict_wrapper, ruby);

            // Build kwargs hash: { sort_keys: true }
            let kwargs = ruby.hash_new();
            let _ = kwargs.aset(ruby.to_symbol("sort_keys"), true.into_value_with(ruby));

            let args = vec![
                "dumps".into_value_with(ruby),
                dict_value,
                kwargs.into_value_with(ruby),
            ];
            let result = wrapper
                .method_missing(&args)
                .expect("dumps with kwargs should succeed");
            let py_result = Obj::<RubyxObject>::try_convert(result)
                .expect("result should be wrapped Python object");
            assert!(
                api.is_string(py_result.as_ptr()),
                "json.dumps should return a string"
            );
            let json_str = api
                .string_to_string(py_result.as_ptr())
                .expect("result should decode as string");
            assert!(
                json_str.contains("\"a\""),
                "JSON should contain key 'a': {}",
                json_str
            );

            drop(wrapper);
            api.decref(py_dict);
            api.decref(json);
        });
    }
    // ========== Async Streaming Integration Tests ==========
    //
    // GIL choreography: with_ruby_python holds both Ruby GVL + Python GIL.
    // AsyncStream::from_python_iterator spawns a worker that needs the GIL.
    // To avoid deadlock: save_thread() releases the GIL before consuming the
    // stream, then restore_thread() re-acquires it for cleanup.

    const PY_EVAL_INPUT: i64 = 258;
    const PY_FILE_INPUT: i64 = 257;

    use crate::python_api::PythonApi;
    use crate::python_ffi::PyObject;

    fn make_globals(api: &PythonApi) -> *mut PyObject {
        let globals = api.dict_new();
        let builtins_key = api.string_from_str("__builtins__");
        let builtins = api
            .import_module("builtins")
            .expect("builtins should exist");
        api.dict_set_item(globals, builtins_key, builtins);
        api.decref(builtins_key);
        api.decref(builtins);
        globals
    }

    #[test]
    #[serial]
    fn test_stream_python_generator() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            // Define a Python generator
            api.run_string(
                "def gen():\n    yield 10\n    yield 20\n    yield 30\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define generator");

            let py_iter = api
                .run_string("gen()", PY_EVAL_INPUT, globals, globals)
                .expect("should create generator iterator");
            assert!(!py_iter.is_null());

            // Release GIL so worker thread can acquire it
            let tstate = api.save_thread();

            let mut stream = crate::stream::AsyncStream::from_python_iterator(py_iter);

            let v1 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1).unwrap(), 10_i64);

            let v2 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v2).unwrap(), 20_i64);

            let v3 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v3).unwrap(), 30_i64);

            assert!(stream.next().is_none());

            // Drop stream before re-acquiring GIL (join waits for worker to release GIL)
            drop(stream);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_stream_is_lazy() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            // Create a large range iterator — if not lazy, would try to materialize all items
            let py_iter = api
                .run_string("iter(range(1000000))", PY_EVAL_INPUT, globals, globals)
                .expect("should create range iterator");

            let tstate = api.save_thread();

            let mut stream = crate::stream::AsyncStream::from_python_iterator(py_iter);

            // Only consume 5 items from a million-item stream
            for expected in 0..5_i64 {
                let val = stream.next().unwrap().unwrap();
                assert_eq!(i64::try_convert(val).unwrap(), expected);
            }

            // Drop without consuming the rest — should not hang or OOM
            let start = std::time::Instant::now();
            drop(stream);
            let elapsed = start.elapsed();

            api.restore_thread(tstate);
            api.decref(globals);

            assert!(
                elapsed < std::time::Duration::from_secs(2),
                "dropping lazy stream should be fast, took {:?}",
                elapsed
            );
        });
    }

    #[test]
    #[serial]
    fn test_stream_cancellation() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            // Infinite generator — only cancellation can stop it
            api.run_string(
                "def infinite():\n    i = 0\n    while True:\n        yield i\n        i += 1\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define infinite generator");

            let py_iter = api
                .run_string("infinite()", PY_EVAL_INPUT, globals, globals)
                .expect("should create infinite iterator");

            let tstate = api.save_thread();

            let mut stream = crate::stream::AsyncStream::from_python_iterator(py_iter);

            // Read a few items
            for expected in 0..3_i64 {
                let val = stream.next().unwrap().unwrap();
                assert_eq!(i64::try_convert(val).unwrap(), expected);
            }

            // Drop triggers cancellation — must not hang on an infinite generator
            let start = std::time::Instant::now();
            drop(stream);
            let elapsed = start.elapsed();

            api.restore_thread(tstate);
            api.decref(globals);

            assert!(
                elapsed < std::time::Duration::from_secs(2),
                "cancelling infinite stream should be fast, took {:?}",
                elapsed
            );
        });
    }

    #[test]
    #[serial]
    fn test_concurrent_streams() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            // Create two separate iterators
            let py_iter1 = api
                .run_string("iter([100, 200, 300])", PY_EVAL_INPUT, globals, globals)
                .expect("should create first iterator");

            let py_iter2 = api
                .run_string("iter([400, 500, 600])", PY_EVAL_INPUT, globals, globals)
                .expect("should create second iterator");

            let tstate = api.save_thread();

            let mut stream1 = crate::stream::AsyncStream::from_python_iterator(py_iter1);
            let mut stream2 = crate::stream::AsyncStream::from_python_iterator(py_iter2);

            // Interleave consumption from both streams
            let v1a = stream1.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1a).unwrap(), 100_i64);

            let v2a = stream2.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v2a).unwrap(), 400_i64);

            let v1b = stream1.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1b).unwrap(), 200_i64);

            let v2b = stream2.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v2b).unwrap(), 500_i64);

            let v1c = stream1.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1c).unwrap(), 300_i64);

            let v2c = stream2.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v2c).unwrap(), 600_i64);

            assert!(stream1.next().is_none());
            assert!(stream2.next().is_none());

            drop(stream1);
            drop(stream2);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_stream_error_propagation() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            // Generator that raises after yielding some values
            api.run_string(
                "def error_gen():\n    yield 1\n    yield 2\n    raise ValueError('test error')\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define error generator");

            let py_iter = api
                .run_string("error_gen()", PY_EVAL_INPUT, globals, globals)
                .expect("should create error generator");

            let tstate = api.save_thread();

            let mut stream = crate::stream::AsyncStream::from_python_iterator(py_iter);

            // First two values succeed
            let v1 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1).unwrap(), 1_i64);

            let v2 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v2).unwrap(), 2_i64);

            // Third call should propagate the error or end the stream
            // (depends on whether iter_next detects the ValueError)
            let v3 = stream.next();
            match v3 {
                Some(Err(err)) => {
                    // Error propagated — good
                    assert!(
                        err.to_string().contains("Error") || err.to_string().contains("error"),
                        "error should contain relevant message, got: {}",
                        err
                    );
                }
                None => {
                    // Stream ended (PyIter_Next returned null with error set,
                    // but error was cleared) — acceptable behavior
                }
                Some(Ok(_)) => {
                    panic!("expected error or end-of-stream after ValueError, got a value");
                }
            }

            drop(stream);
            api.restore_thread(tstate);

            // Clear any lingering Python error
            if api.has_error() {
                api.clear_error();
            }

            api.decref(globals);
        });
    }

    // ========== AsyncGeneratorStream Integration Tests ==========
    //
    // These tests exercise AsyncGeneratorStream::from_python_object, which is
    // used by both Rubyx.stream() (PythonAdapter) and Rubyx.async_stream()
    // (RustDriving). They verify the full pipeline: Python object → detection
    // → strategy selection → Iterator → magnus::Value.
    //
    // GIL choreography: with_ruby_python holds the GIL via ensure_gil.
    // from_python_object also calls ensure_gil/release_gil internally (re-entrant).
    // IMPORTANT: call from_python_object WHILE GIL is held, then save_thread()
    // AFTER to release the GIL so the worker thread can proceed.

    use crate::async_gen::{AsyncGeneratorStream, AsyncStrategy};

    #[test]
    #[serial]
    fn test_async_gen_stream_sync_iterable_via_python_adapter() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            let py_obj = api
                .run_string("range(4)", PY_EVAL_INPUT, globals, globals)
                .expect("should create range object");

            // Create stream while GIL is held (from_python_object is re-entrant)
            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter)
                    .expect("should create stream from sync iterable");

            // Release GIL so worker thread can acquire it
            let tstate = api.save_thread();

            for expected in 0..4_i64 {
                let val = stream.next().unwrap().unwrap();
                assert_eq!(i64::try_convert(val).unwrap(), expected);
            }
            assert!(stream.next().is_none());

            drop(stream);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_sync_generator_via_python_adapter() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "def countdown(n):\n    while n > 0:\n        yield n\n        n -= 1\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define generator");

            let py_obj = api
                .run_string("countdown(3)", PY_EVAL_INPUT, globals, globals)
                .expect("should create generator");

            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter)
                    .expect("should create stream from sync generator");

            let tstate = api.save_thread();

            let v1 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1).unwrap(), 3_i64);
            let v2 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v2).unwrap(), 2_i64);
            let v3 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v3).unwrap(), 1_i64);
            assert!(stream.next().is_none());

            drop(stream);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_async_generator_via_python_adapter() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "async def async_range(n):\n    for i in range(n):\n        yield i\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async generator");

            let py_obj = api
                .run_string("async_range(3)", PY_EVAL_INPUT, globals, globals)
                .expect("should create async generator instance");

            // PythonAdapter wraps async gen → sync iter via AsyncToSync adapter
            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter)
                    .expect("should create stream from async generator via adapter");

            let tstate = api.save_thread();

            for expected in 0..3_i64 {
                let val = stream.next().unwrap().unwrap();
                assert_eq!(i64::try_convert(val).unwrap(), expected);
            }
            assert!(stream.next().is_none());

            drop(stream);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_async_generator_via_rust_driving() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "async def async_squares(n):\n    for i in range(n):\n        yield i * i\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async generator");

            let py_obj = api
                .run_string("async_squares(4)", PY_EVAL_INPUT, globals, globals)
                .expect("should create async generator instance");

            // RustDriving uses Rust-side event loop to drive __anext__() coroutines
            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::RustDriving)
                    .expect("should create stream from async generator via rust driving");

            let tstate = api.save_thread();

            let expected = [0_i64, 1, 4, 9];
            for exp in expected {
                let val = stream.next().unwrap().unwrap();
                assert_eq!(i64::try_convert(val).unwrap(), exp);
            }
            assert!(stream.next().is_none());

            drop(stream);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_async_with_arguments() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            // Async generator that takes arguments
            api.run_string(
                "async def async_countdown(start, step=1):\n    n = start\n    while n > 0:\n        yield n\n        n -= step\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async generator with args");

            // Call with arguments — the async gen is already instantiated
            let py_obj = api
                .run_string(
                    "async_countdown(6, step=2)",
                    PY_EVAL_INPUT,
                    globals,
                    globals,
                )
                .expect("should create async generator with args");

            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter)
                    .expect("should create stream from async generator with args");

            let tstate = api.save_thread();

            let expected = [6_i64, 4, 2];
            for exp in expected {
                let val = stream.next().unwrap().unwrap();
                assert_eq!(i64::try_convert(val).unwrap(), exp);
            }
            assert!(stream.next().is_none());

            drop(stream);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_async_error_propagation() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "async def async_error_gen():\n    yield 1\n    raise ValueError('async error')\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async error generator");

            let py_obj = api
                .run_string("async_error_gen()", PY_EVAL_INPUT, globals, globals)
                .expect("should create async error generator");

            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter)
                    .expect("should create stream");

            let tstate = api.save_thread();

            // First value succeeds
            let v1 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1).unwrap(), 1_i64);

            // Second call should propagate the error
            let v2 = stream.next();
            match v2 {
                Some(Err(err)) => {
                    assert!(
                        err.to_string().contains("ValueError")
                            || err.to_string().contains("async error"),
                        "expected ValueError, got: {}",
                        err
                    );
                }
                None => {
                    // Stream ended — acceptable if adapter swallows the error
                }
                Some(Ok(_)) => {
                    panic!("expected error or end-of-stream after ValueError");
                }
            }

            drop(stream);
            api.restore_thread(tstate);

            if api.has_error() {
                api.clear_error();
            }
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_rust_driving_error_propagation() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "async def async_boom():\n    yield 42\n    raise RuntimeError('boom from async')\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async boom generator");

            let py_obj = api
                .run_string("async_boom()", PY_EVAL_INPUT, globals, globals)
                .expect("should create async boom generator");

            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::RustDriving)
                    .expect("should create stream via rust driving");

            let tstate = api.save_thread();

            // First value succeeds
            let v1 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1).unwrap(), 42_i64);

            // Second call should propagate the error
            let v2 = stream.next();
            match v2 {
                Some(Err(err)) => {
                    assert!(
                        err.to_string().contains("RuntimeError")
                            || err.to_string().contains("boom from async"),
                        "expected RuntimeError, got: {}",
                        err
                    );
                }
                None => {
                    // Stream ended — acceptable
                }
                Some(Ok(_)) => {
                    panic!("expected error or end-of-stream after RuntimeError");
                }
            }

            drop(stream);
            api.restore_thread(tstate);

            if api.has_error() {
                api.clear_error();
            }
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_non_iterable_returns_error() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            let py_obj = api
                .run_string("42", PY_EVAL_INPUT, globals, globals)
                .expect("should create integer");

            // from_python_object handles GIL internally — no save_thread needed
            // since no worker thread is spawned on error
            let result =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter);
            match result {
                Err(msg) => {
                    assert!(
                        msg.contains("not iterable"),
                        "error should mention 'not iterable', got: {msg}"
                    );
                }
                Ok(_) => panic!("non-iterable should return error"),
            }

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_empty_async_generator() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "async def async_empty():\n    return\n    yield\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define empty async generator");

            let py_obj = api
                .run_string("async_empty()", PY_EVAL_INPUT, globals, globals)
                .expect("should create empty async generator");

            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter)
                    .expect("should create stream from empty async gen");

            let tstate = api.save_thread();

            assert!(
                stream.next().is_none(),
                "empty async gen should yield nothing"
            );

            drop(stream);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_cancellation_mid_iteration() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            // Async generator that yields many values
            api.run_string(
                "async def async_infinite():\n    i = 0\n    while True:\n        yield i\n        i += 1\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define infinite async generator");

            let py_obj = api
                .run_string("async_infinite()", PY_EVAL_INPUT, globals, globals)
                .expect("should create infinite async generator");

            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter)
                    .expect("should create stream");

            let tstate = api.save_thread();

            // Read a few items
            for expected in 0..3_i64 {
                let val = stream.next().unwrap().unwrap();
                assert_eq!(i64::try_convert(val).unwrap(), expected);
            }

            // Drop should cancel cleanly without hanging
            let start = std::time::Instant::now();
            drop(stream);
            let elapsed = start.elapsed();

            api.restore_thread(tstate);
            api.decref(globals);

            assert!(
                elapsed < std::time::Duration::from_secs(2),
                "cancelling async generator should be fast, took {:?}",
                elapsed
            );
        });
    }

    #[test]
    #[serial]
    fn test_async_gen_stream_yields_mixed_types() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "async def async_mixed():\n    yield 42\n    yield 'hello'\n    yield 3.14\n    yield True\n    yield None\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define mixed-type async generator");

            let py_obj = api
                .run_string("async_mixed()", PY_EVAL_INPUT, globals, globals)
                .expect("should create mixed-type async generator");

            let mut stream =
                AsyncGeneratorStream::from_python_object(py_obj, AsyncStrategy::PythonAdapter)
                    .expect("should create stream");

            let tstate = api.save_thread();

            let v1 = stream.next().unwrap().unwrap();
            assert_eq!(i64::try_convert(v1).unwrap(), 42);

            let v2 = stream.next().unwrap().unwrap();
            assert_eq!(String::try_convert(v2).unwrap(), "hello");

            let v3 = stream.next().unwrap().unwrap();
            assert!((f64::try_convert(v3).unwrap() - 3.14).abs() < 0.001);

            let v4 = stream.next().unwrap().unwrap();
            assert!(bool::try_convert(v4).unwrap());

            let v5 = stream.next().unwrap().unwrap();
            assert!(magnus::value::ReprValue::is_nil(v5));

            assert!(stream.next().is_none());

            drop(stream);
            api.restore_thread(tstate);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_inject_sys_paths_adds_paths() {
        with_ruby_python(|ruby, api| {
            let paths = ruby.ary_new();
            paths
                .push(ruby.str_new("/tmp/rubyx_inject_test_a"))
                .unwrap();
            paths
                .push(ruby.str_new("/tmp/rubyx_inject_test_b"))
                .unwrap();

            super::inject_sys_paths(api, &paths).expect("inject_sys_paths should succeed");

            // Verify paths were added
            let globals = test_make_globals(api);
            let result = api
                .run_string(
                    "'/tmp/rubyx_inject_test_a' in __import__('sys').path",
                    258,
                    globals,
                    std::ptr::null_mut(),
                )
                .unwrap();
            assert!(api.is_true(result), "path a should be in sys.path");
            api.decref(result);

            let result = api
                .run_string(
                    "'/tmp/rubyx_inject_test_b' in __import__('sys').path",
                    258,
                    globals,
                    std::ptr::null_mut(),
                )
                .unwrap();
            assert!(api.is_true(result), "path b should be in sys.path");
            api.decref(result);

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_inject_sys_paths_empty_array() {
        with_ruby_python(|ruby, api| {
            let sys_module = api.import_module("sys").expect("should import sys");
            let sys_path = api.object_get_attr_string(sys_module, "path");
            let size_before = unsafe { (api.py_list_size)(sys_path) };
            api.decref(sys_path);
            api.decref(sys_module);

            let paths = ruby.ary_new();
            super::inject_sys_paths(api, &paths).expect("empty inject should succeed");

            let sys_module = api.import_module("sys").expect("should import sys");
            let sys_path = api.object_get_attr_string(sys_module, "path");
            let size_after = unsafe { (api.py_list_size)(sys_path) };
            api.decref(sys_path);
            api.decref(sys_module);

            assert_eq!(
                size_before, size_after,
                "empty inject should not change sys.path"
            );
        });
    }

    #[test]
    #[serial]
    fn test_inject_sys_paths_enables_local_module_import() {
        with_ruby_python(|ruby, api| {
            // Create a temp module
            let tmp_dir = std::env::temp_dir().join("rubyx_inject_import_test");
            std::fs::create_dir_all(&tmp_dir).unwrap();
            std::fs::write(
                tmp_dir.join("rubyx_inject_calc.py"),
                "RESULT = 100\ndef double(x): return x * 2\n",
            )
            .unwrap();

            let paths = ruby.ary_new();
            paths.push(ruby.str_new(tmp_dir.to_str().unwrap())).unwrap();
            super::inject_sys_paths(api, &paths).expect("inject should succeed");

            // Import the module
            let module = api
                .import_module("rubyx_inject_calc")
                .expect("should import module from injected path");

            let result_attr = api.object_get_attr_string(module, "RESULT");
            assert!(!result_attr.is_null());
            assert_eq!(api.long_to_i64(result_attr), 100);

            api.decref(result_attr);
            api.decref(module);
            let _ = std::fs::remove_dir_all(&tmp_dir);
        });
    }

    #[test]
    #[serial]
    fn test_inject_sys_paths_preserves_existing() {
        with_ruby_python(|ruby, api| {
            // Get current sys.path size
            let sys_module = api.import_module("sys").expect("should import sys");
            let sys_path = api.object_get_attr_string(sys_module, "path");
            let size_before = unsafe { (api.py_list_size)(sys_path) };
            api.decref(sys_path);
            api.decref(sys_module);

            // Inject 3 paths
            let paths = ruby.ary_new();
            paths.push(ruby.str_new("/tmp/p1")).unwrap();
            paths.push(ruby.str_new("/tmp/p2")).unwrap();
            paths.push(ruby.str_new("/tmp/p3")).unwrap();
            super::inject_sys_paths(api, &paths).unwrap();

            let sys_module = api.import_module("sys").expect("should import sys");
            let sys_path = api.object_get_attr_string(sys_module, "path");
            let size_after = unsafe { (api.py_list_size)(sys_path) };
            api.decref(sys_path);
            api.decref(sys_module);

            assert_eq!(
                size_after,
                size_before + 3,
                "sys.path should grow by exactly 3"
            );
        });
    }

    #[test]
    #[serial]
    fn test_api_not_initialized_gives_clear_message() {
        // Skip if Python isn't available (the test harness couldn't find libpython)
        if crate::API.get().is_none() {
            println!("Skipping: Python not available");
            return;
        }
        // If we get here, API was initialized — verify it works
        let api = crate::api();
        assert!(api.is_initialized());
    }

    #[test]
    #[serial]
    fn test_rubyx_initialized_returns_true_after_init() {
        if crate::API.get().is_none() {
            println!("Skipping: Python not available");
            return;
        }
        assert!(crate::rubyx_initialized());
    }

    // ========== to_s tests ==========

    #[test]
    #[serial]
    fn test_to_s_integer() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        let wrapper = RubyxObject::new(py_int, api).unwrap();
        let result = wrapper.to_s().expect("to_s should succeed");
        assert_eq!(result, "42");

        drop(wrapper);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_to_s_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("hello world");
        let wrapper = RubyxObject::new(py_str, api).unwrap();
        let result = wrapper.to_s().expect("to_s should succeed");
        assert_eq!(result, "hello world");

        drop(wrapper);
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_to_s_float() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_float = api.float_from_f64(3.14);
        let wrapper = RubyxObject::new(py_float, api).unwrap();
        let result = wrapper.to_s().expect("to_s should succeed");
        assert!(
            result.starts_with("3.14"),
            "to_s of 3.14 should start with '3.14', got: {result}"
        );

        drop(wrapper);
        api.decref(py_float);
    }

    #[test]
    #[serial]
    fn test_to_s_bool() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_true = api.bool_from_i64(1);
        let wrapper = RubyxObject::new(py_true, api).unwrap();
        let result = wrapper.to_s().expect("to_s should succeed");
        assert_eq!(result, "True");

        drop(wrapper);
        api.decref(py_true);
    }

    #[test]
    #[serial]
    fn test_to_s_none() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_none = api.py_none;
        api.incref(py_none);
        let wrapper = RubyxObject::new(py_none, api).unwrap();
        let result = wrapper.to_s().expect("to_s should succeed");
        assert_eq!(result, "None");

        drop(wrapper);
    }

    #[test]
    #[serial]
    fn test_to_s_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let list = unsafe { (api.py_list_new)(3) };
        unsafe {
            (api.py_list_set_item)(list, 0, api.long_from_i64(1));
            (api.py_list_set_item)(list, 1, api.long_from_i64(2));
            (api.py_list_set_item)(list, 2, api.long_from_i64(3));
        }
        let wrapper = RubyxObject::new(list, api).unwrap();
        let result = wrapper.to_s().expect("to_s should succeed");
        assert_eq!(result, "[1, 2, 3]");

        drop(wrapper);
        api.decref(list);
    }

    // ========== inspect tests ==========

    #[test]
    #[serial]
    fn test_inspect_integer() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        let wrapper = RubyxObject::new(py_int, api).unwrap();
        let result = wrapper.inspect().expect("inspect should succeed");
        assert_eq!(result, "42");

        drop(wrapper);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_inspect_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("hello");
        let wrapper = RubyxObject::new(py_str, api).unwrap();
        let result = wrapper.inspect().expect("inspect should succeed");
        // Python repr of a string includes quotes
        assert_eq!(result, "'hello'");

        drop(wrapper);
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_inspect_none() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_none = api.py_none;
        api.incref(py_none);
        let wrapper = RubyxObject::new(py_none, api).unwrap();
        let result = wrapper.inspect().expect("inspect should succeed");
        assert_eq!(result, "None");

        drop(wrapper);
    }

    #[test]
    #[serial]
    fn test_inspect_dict() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let dict = api.dict_new();
        let key = api.string_from_str("x");
        let val = api.long_from_i64(1);
        api.dict_set_item(dict, key, val);
        api.decref(key);
        api.decref(val);

        let wrapper = RubyxObject::new(dict, api).unwrap();
        let result = wrapper.inspect().expect("inspect should succeed");
        assert_eq!(result, "{'x': 1}");

        drop(wrapper);
        api.decref(dict);
    }

    // ========== to_ruby tests ==========

    #[test]
    #[serial]
    fn test_to_ruby_integer() {
        with_ruby_python(|_ruby, api| {
            let py_int = api.long_from_i64(42);
            let wrapper = RubyxObject::new(py_int, api).unwrap();
            let result = wrapper.to_ruby().expect("to_ruby should succeed");
            let val = i64::try_convert(result).expect("should convert to i64");
            assert_eq!(val, 42);

            drop(wrapper);
            api.decref(py_int);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_string() {
        with_ruby_python(|_ruby, api| {
            let py_str = api.string_from_str("hello");
            let wrapper = RubyxObject::new(py_str, api).unwrap();
            let result = wrapper.to_ruby().expect("to_ruby should succeed");
            let val = String::try_convert(result).expect("should convert to String");
            assert_eq!(val, "hello");

            drop(wrapper);
            api.decref(py_str);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_float() {
        with_ruby_python(|_ruby, api| {
            let py_float = api.float_from_f64(3.14);
            let wrapper = RubyxObject::new(py_float, api).unwrap();
            let result = wrapper.to_ruby().expect("to_ruby should succeed");
            let val = f64::try_convert(result).expect("should convert to f64");
            assert!((val - 3.14).abs() < 0.001);

            drop(wrapper);
            api.decref(py_float);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_bool_true() {
        with_ruby_python(|_ruby, api| {
            let py_true = api.bool_from_i64(1);
            let wrapper = RubyxObject::new(py_true, api).unwrap();
            let result = wrapper.to_ruby().expect("to_ruby should succeed");
            let val = bool::try_convert(result).expect("should convert to bool");
            assert!(val);

            drop(wrapper);
            api.decref(py_true);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_bool_false() {
        with_ruby_python(|_ruby, api| {
            let py_false = api.bool_from_i64(0);
            let wrapper = RubyxObject::new(py_false, api).unwrap();
            let result = wrapper.to_ruby().expect("to_ruby should succeed");
            let val = bool::try_convert(result).expect("should convert to bool");
            assert!(!val);

            drop(wrapper);
            api.decref(py_false);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_none_returns_nil() {
        with_ruby_python(|_ruby, api| {
            let py_none = api.py_none;
            api.incref(py_none);
            let wrapper = RubyxObject::new(py_none, api).unwrap();
            let result = wrapper.to_ruby().expect("to_ruby should succeed");
            assert!(
                magnus::value::ReprValue::is_nil(result),
                "Python None should convert to Ruby nil"
            );

            drop(wrapper);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_list() {
        with_ruby_python(|_ruby, api| {
            let list = unsafe { (api.py_list_new)(3) };
            unsafe {
                (api.py_list_set_item)(list, 0, api.long_from_i64(10));
                (api.py_list_set_item)(list, 1, api.long_from_i64(20));
                (api.py_list_set_item)(list, 2, api.long_from_i64(30));
            }
            let wrapper = RubyxObject::new(list, api).unwrap();
            let result = wrapper.to_ruby().expect("to_ruby should succeed");

            // Result should be a Ruby array
            let arr = magnus::RArray::try_convert(result).expect("should be an Array");
            assert_eq!(arr.len(), 3);
            assert_eq!(
                i64::try_convert(arr.entry::<magnus::Value>(0).unwrap()).unwrap(),
                10
            );
            assert_eq!(
                i64::try_convert(arr.entry::<magnus::Value>(1).unwrap()).unwrap(),
                20
            );
            assert_eq!(
                i64::try_convert(arr.entry::<magnus::Value>(2).unwrap()).unwrap(),
                30
            );

            drop(wrapper);
            api.decref(list);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_dict() {
        with_ruby_python(|_ruby, api| {
            let dict = api.dict_new();
            let key = api.string_from_str("name");
            let val = api.string_from_str("rubyx");
            api.dict_set_item(dict, key, val);
            api.decref(key);
            api.decref(val);

            let wrapper = RubyxObject::new(dict, api).unwrap();
            let result = wrapper.to_ruby().expect("to_ruby should succeed");

            // Result should be a Ruby hash
            let hash = magnus::RHash::try_convert(result).expect("should be a Hash");
            let name: String = hash
                .aref::<_, magnus::Value>("name")
                .and_then(|v| String::try_convert(v))
                .expect("should have 'name' key");
            assert_eq!(name, "rubyx");

            drop(wrapper);
            api.decref(dict);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_wraps_module_via_pyobjectref() {
        with_ruby_python(|_ruby, api| {
            // Modules have __dict__ and are not callable, so python_to_sendable
            // returns PyObjectRef → wrapped as RubyxObject.
            let module = api.import_module("sys").expect("sys should import");
            let wrapper = RubyxObject::new(module, api).unwrap();
            let result = wrapper.to_ruby();
            assert!(
                result.is_ok(),
                "module should wrap as RubyxObject via PyObjectRef"
            );

            drop(wrapper);
            api.decref(module);
        });
    }

    // ========== to_s / inspect difference ==========

    #[test]
    #[serial]
    fn test_to_s_vs_inspect_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("hello");
        let wrapper = RubyxObject::new(py_str, api).unwrap();

        // to_s returns the string value (Python str())
        let to_s_result = wrapper.to_s().expect("to_s should succeed");
        assert_eq!(to_s_result, "hello");

        // inspect returns the repr (Python repr(), with quotes)
        let inspect_result = wrapper.inspect().expect("inspect should succeed");
        assert_eq!(inspect_result, "'hello'");

        drop(wrapper);
        api.decref(py_str);
    }

    // ========== Rubyx::Future / async_await tests ==========

    /// Spin-wait then call value() — ensures the fast path is hit in tests,
    /// avoiding rb_thread_call_without_gvl which deadlocks in embedded Ruby.
    fn test_future_value(
        future: &crate::future::RubyxFuture,
    ) -> Result<magnus::Value, magnus::Error> {
        while !future.is_ready() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        future.value()
    }

    /// Helper: define an async function in globals, create coroutine,
    /// release GIL, run future, restore GIL, return result.
    fn run_future_test(
        api: &'static PythonApi,
        func_def: &str,
        call_expr: &str,
    ) -> Result<magnus::Value, magnus::Error> {
        let globals = make_globals(api);
        api.run_string(func_def, PY_FILE_INPUT, globals, globals)
            .expect("should define async function");

        let coroutine = api
            .run_string(call_expr, PY_EVAL_INPUT, globals, globals)
            .expect("should create coroutine");

        let tstate = api.save_thread();
        let future = crate::future::RubyxFuture::from_coroutine(coroutine, api);
        let result = test_future_value(&future);
        drop(future);
        api.restore_thread(tstate);

        api.decref(coroutine);
        api.decref(globals);
        result
    }

    #[test]
    #[serial]
    fn test_future_from_async_coroutine() {
        with_ruby_python(|_ruby, api| {
            let result = run_future_test(
                api,
                "import asyncio\nasync def simple(): return 42\n",
                "simple()",
            )
            .expect("future should resolve");
            assert_eq!(i64::try_convert(result).unwrap(), 42);
        });
    }

    #[test]
    #[serial]
    fn test_future_from_async_returning_string() {
        with_ruby_python(|_ruby, api| {
            let result = run_future_test(
                api,
                "import asyncio\nasync def greet(): return 'hello async'\n",
                "greet()",
            )
            .expect("future should resolve");
            assert_eq!(String::try_convert(result).unwrap(), "hello async");
        });
    }

    #[test]
    #[serial]
    fn test_future_from_async_returning_list() {
        with_ruby_python(|_ruby, api| {
            let result = run_future_test(
                api,
                "import asyncio\nasync def get_list(): return [10, 20, 30]\n",
                "get_list()",
            )
            .expect("future should resolve");
            let arr = magnus::RArray::try_convert(result).expect("should be array");
            assert_eq!(arr.len(), 3);
            assert_eq!(
                i64::try_convert(arr.entry::<magnus::Value>(0).unwrap()).unwrap(),
                10
            );
            assert_eq!(
                i64::try_convert(arr.entry::<magnus::Value>(1).unwrap()).unwrap(),
                20
            );
            assert_eq!(
                i64::try_convert(arr.entry::<magnus::Value>(2).unwrap()).unwrap(),
                30
            );
        });
    }

    #[test]
    #[serial]
    fn test_future_from_async_returning_none() {
        with_ruby_python(|_ruby, api| {
            let result = run_future_test(api, "import asyncio\nasync def noop(): pass\n", "noop()")
                .expect("future should resolve");
            assert!(
                magnus::value::ReprValue::is_nil(result),
                "None should become nil"
            );
        });
    }

    #[test]
    #[serial]
    fn test_future_propagates_async_error() {
        with_ruby_python(|_ruby, api| {
            let result = run_future_test(
                api,
                "import asyncio\nasync def boom(): raise ValueError('async boom')\n",
                "boom()",
            );
            assert!(result.is_err(), "async error should propagate");
            let err_msg = format!("{}", result.unwrap_err());
            assert!(err_msg.contains("async boom"), "got: {}", err_msg);
        });
    }

    #[test]
    #[serial]
    fn test_future_with_await_in_coroutine() {
        with_ruby_python(|_ruby, api| {
            let result = run_future_test(
                api,
                "import asyncio\nasync def chained():\n    await asyncio.sleep(0.01)\n    return 'done'\n",
                "chained()",
            )
            .expect("future should resolve");
            assert_eq!(String::try_convert(result).unwrap(), "done");
        });
    }

    #[test]
    #[serial]
    fn test_future_sequential_multiple() {
        with_ruby_python(|_ruby, api| {
            let r1 = run_future_test(
                api,
                "import asyncio\nasync def add(a, b): return a + b\n",
                "add(1, 2)",
            )
            .expect("future1 should resolve");
            assert_eq!(i64::try_convert(r1).unwrap(), 3);

            let r2 = run_future_test(
                api,
                "import asyncio\nasync def add2(a, b): return a + b\n",
                "add2(10, 20)",
            )
            .expect("future2 should resolve");
            assert_eq!(i64::try_convert(r2).unwrap(), 30);
        });
    }

    #[test]
    #[serial]
    fn test_future_drop_joins_thread() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);
            api.run_string(
                "import asyncio\nasync def slow(): await asyncio.sleep(0.05); return 1\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let coroutine = api
                .run_string("slow()", PY_EVAL_INPUT, globals, globals)
                .expect("should create coroutine");

            let tstate = api.save_thread();
            let future = crate::future::RubyxFuture::from_coroutine(coroutine, api);
            drop(future); // Should not hang or crash
            api.restore_thread(tstate);

            api.decref(coroutine);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_blocking_await_returns_rubyx_object() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);
            api.run_string(
                "import asyncio\nasync def get_val(): return 99\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let coroutine = api
                .run_string("get_val()", PY_EVAL_INPUT, globals, globals)
                .expect("should create coroutine");

            // Manually do what rubyx_await does, but with proper GIL management
            // for the test environment (can't call rubyx_await directly because
            // its ensure_gil/release_gil nests incorrectly with with_ruby_python's GIL)
            let future = crate::future::RubyxFuture::from_coroutine(coroutine, api);

            // Release GIL so the background thread can acquire it
            let tstate = api.save_thread();
            let result = test_future_value(&future).expect("await should succeed");
            drop(future);
            api.restore_thread(tstate);

            assert_eq!(i64::try_convert(result).unwrap(), 99);

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_await_eval_with_globals() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);
            api.run_string(
                "import asyncio\nasync def double(n): return n * 2\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let future = crate::eval::await_eval_with_globals("double(21)", globals, api)
                .expect("await_eval should succeed");

            // Release GIL so background thread can run asyncio
            let tstate = api.save_thread();
            let result = test_future_value(&future).expect("future should resolve");
            drop(future);
            api.restore_thread(tstate);

            assert_eq!(i64::try_convert(result).unwrap(), 42);

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_await_eval_with_globals_error() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);
            api.run_string(
                "import asyncio\nasync def fail(): raise RuntimeError('eval boom')\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let future_result = crate::eval::await_eval_with_globals("fail()", globals, api);
            match future_result {
                Err(_) => {} // eval itself failed — error propagated
                Ok(future) => {
                    // Eval succeeded but asyncio.run should fail
                    let tstate = api.save_thread();
                    let result = test_future_value(&future);
                    api.restore_thread(tstate);
                    assert!(result.is_err(), "should propagate error");
                }
            }

            api.decref(globals);
        });
    }

    // ========== eval_with_globals tests ==========

    #[test]
    #[serial]
    fn test_eval_with_globals_simple_addition() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            // Inject x=10 and y=20 into globals
            let py_x = crate::rubyx_object::ruby_to_python(10_i64.into_value_with(ruby), api)
                .expect("should convert x");
            let py_y = crate::rubyx_object::ruby_to_python(20_i64.into_value_with(ruby), api)
                .expect("should convert y");
            let key_x = api.string_from_str("x");
            let key_y = api.string_from_str("y");
            api.dict_set_item(globals, key_x, py_x);
            api.dict_set_item(globals, key_y, py_y);
            api.decref(key_x);
            api.decref(key_y);
            api.decref(py_x);
            api.decref(py_y);

            let result =
                crate::eval::eval_with_globals("x + y", globals, api).expect("eval should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 30);

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_eval_with_globals_string_interpolation() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            let py_name = crate::rubyx_object::ruby_to_python("Alice".into_value_with(ruby), api)
                .expect("should convert name");
            let key = api.string_from_str("name");
            api.dict_set_item(globals, key, py_name);
            api.decref(key);
            api.decref(py_name);

            let result = crate::eval::eval_with_globals("f'Hello, {name}!'", globals, api)
                .expect("eval should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(
                api.string_to_string(obj.as_ptr()),
                Some("Hello, Alice!".to_string())
            );

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_eval_with_globals_list_operations() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            let arr = magnus::RArray::new();
            arr.push(1_i64.into_value_with(ruby)).unwrap();
            arr.push(2_i64.into_value_with(ruby)).unwrap();
            arr.push(3_i64.into_value_with(ruby)).unwrap();
            let py_items = crate::rubyx_object::ruby_to_python(arr.into_value_with(ruby), api)
                .expect("should convert list");
            let key = api.string_from_str("items");
            api.dict_set_item(globals, key, py_items);
            api.decref(key);
            api.decref(py_items);

            let result = crate::eval::eval_with_globals("sum(items)", globals, api)
                .expect("eval should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 6);

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_eval_with_globals_dict_access() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            let hash = magnus::RHash::new();
            hash.aset(ruby.sym_new("a"), 100_i64.into_value_with(ruby))
                .unwrap();
            hash.aset(ruby.sym_new("b"), 200_i64.into_value_with(ruby))
                .unwrap();
            let py_dict = crate::rubyx_object::ruby_to_python(hash.into_value_with(ruby), api)
                .expect("should convert dict");
            let key = api.string_from_str("data");
            api.dict_set_item(globals, key, py_dict);
            api.decref(key);
            api.decref(py_dict);

            let result = crate::eval::eval_with_globals("data['a'] + data['b']", globals, api)
                .expect("eval should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 300);

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_eval_with_globals_undefined_variable_errors() {
        with_ruby_python(|_ruby, api| {
            let globals = make_globals(api);

            let result = crate::eval::eval_with_globals("undefined_var", globals, api);
            assert!(result.is_err(), "should fail for undefined variable");

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_eval_with_globals_multiline_with_globals() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            let py_n = crate::rubyx_object::ruby_to_python(5_i64.into_value_with(ruby), api)
                .expect("should convert n");
            let key = api.string_from_str("n");
            api.dict_set_item(globals, key, py_n);
            api.decref(key);
            api.decref(py_n);

            let code = "result = 0\nfor i in range(n):\n    result += i\nresult";
            let result = crate::eval::eval_with_globals(code, globals, api)
                .expect("multiline eval should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 10); // 0+1+2+3+4 = 10

            api.decref(globals);
        });
    }

    // ========== await_with_globals tests ==========

    #[test]
    #[serial]
    fn test_await_with_globals_simple() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            // Define async function
            api.run_string(
                "import asyncio\nasync def multiply(a, b): return a * b\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            // Inject globals
            let py_a = crate::rubyx_object::ruby_to_python(7_i64.into_value_with(ruby), api)
                .expect("should convert a");
            let py_b = crate::rubyx_object::ruby_to_python(6_i64.into_value_with(ruby), api)
                .expect("should convert b");
            let key_a = api.string_from_str("a");
            let key_b = api.string_from_str("b");
            api.dict_set_item(globals, key_a, py_a);
            api.dict_set_item(globals, key_b, py_b);
            api.decref(key_a);
            api.decref(key_b);
            api.decref(py_a);
            api.decref(py_b);

            let future = crate::eval::await_eval_with_globals("multiply(a, b)", globals, api)
                .expect("await should succeed");

            let tstate = api.save_thread();
            let result = test_future_value(&future).expect("future should resolve");
            drop(future);
            api.restore_thread(tstate);

            assert_eq!(i64::try_convert(result).unwrap(), 42);

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_await_with_globals_string_result() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "import asyncio\nasync def greet(who): return f'hi {who}'\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let py_who = crate::rubyx_object::ruby_to_python("world".into_value_with(ruby), api)
                .expect("should convert who");
            let key = api.string_from_str("who");
            api.dict_set_item(globals, key, py_who);
            api.decref(key);
            api.decref(py_who);

            let future = crate::eval::await_eval_with_globals("greet(who)", globals, api)
                .expect("await should succeed");

            let tstate = api.save_thread();
            let result = test_future_value(&future).expect("future should resolve");
            drop(future);
            api.restore_thread(tstate);

            let s: String = TryConvert::try_convert(result).unwrap();
            assert_eq!(s, "hi world");

            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_await_with_globals_error_propagation() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "import asyncio\nasync def check(val):\n    if val < 0: raise ValueError('negative')\n    return val\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let py_val = crate::rubyx_object::ruby_to_python((-1_i64).into_value_with(ruby), api)
                .expect("should convert val");
            let key = api.string_from_str("val");
            api.dict_set_item(globals, key, py_val);
            api.decref(key);
            api.decref(py_val);

            let future_result = crate::eval::await_eval_with_globals("check(val)", globals, api);
            match future_result {
                Err(_) => {} // eval itself failed
                Ok(future) => {
                    let tstate = api.save_thread();
                    let result = test_future_value(&future);
                    api.restore_thread(tstate);
                    assert!(result.is_err(), "should propagate ValueError");
                }
            }

            api.decref(globals);
        });
    }

    // ========== async_await_with_globals tests ==========

    #[test]
    #[serial]
    fn test_async_await_with_globals_future() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "import asyncio\nasync def add(x, y): return x + y\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let py_x = crate::rubyx_object::ruby_to_python(15_i64.into_value_with(ruby), api)
                .expect("should convert x");
            let py_y = crate::rubyx_object::ruby_to_python(27_i64.into_value_with(ruby), api)
                .expect("should convert y");
            let key_x = api.string_from_str("x");
            let key_y = api.string_from_str("y");
            api.dict_set_item(globals, key_x, py_x);
            api.dict_set_item(globals, key_y, py_y);
            api.decref(key_x);
            api.decref(key_y);
            api.decref(py_x);
            api.decref(py_y);

            // Create coroutine
            let coroutine = api
                .run_string("add(x, y)", PY_EVAL_INPUT, globals, globals)
                .expect("should create coroutine");

            // Release GIL so the background thread can acquire it
            let tstate = api.save_thread();
            let future = crate::future::RubyxFuture::from_coroutine(coroutine, api);
            let result = test_future_value(&future).expect("future should resolve");
            drop(future);
            api.restore_thread(tstate);

            assert_eq!(i64::try_convert(result).unwrap(), 42);

            api.decref(coroutine);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_await_with_globals_future_string() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "import asyncio\nasync def fmt(prefix, val): return f'{prefix}: {val}'\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let py_prefix =
                crate::rubyx_object::ruby_to_python("result".into_value_with(ruby), api)
                    .expect("should convert prefix");
            let py_val = crate::rubyx_object::ruby_to_python(99_i64.into_value_with(ruby), api)
                .expect("should convert val");
            let key_p = api.string_from_str("prefix");
            let key_v = api.string_from_str("val");
            api.dict_set_item(globals, key_p, py_prefix);
            api.dict_set_item(globals, key_v, py_val);
            api.decref(key_p);
            api.decref(key_v);
            api.decref(py_prefix);
            api.decref(py_val);

            let coroutine = api
                .run_string("fmt(prefix, val)", PY_EVAL_INPUT, globals, globals)
                .expect("should create coroutine");

            let tstate = api.save_thread();
            let future = crate::future::RubyxFuture::from_coroutine(coroutine, api);
            let result = test_future_value(&future).expect("future should resolve");
            drop(future);
            api.restore_thread(tstate);

            assert_eq!(String::try_convert(result).unwrap(), "result: 99");

            api.decref(coroutine);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_async_await_with_globals_error() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);

            api.run_string(
                "import asyncio\nasync def div(a, b): return a / b\n",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .expect("should define async function");

            let py_a = crate::rubyx_object::ruby_to_python(10_i64.into_value_with(ruby), api)
                .expect("should convert a");
            let py_b = crate::rubyx_object::ruby_to_python(0_i64.into_value_with(ruby), api)
                .expect("should convert b");
            let key_a = api.string_from_str("a");
            let key_b = api.string_from_str("b");
            api.dict_set_item(globals, key_a, py_a);
            api.dict_set_item(globals, key_b, py_b);
            api.decref(key_a);
            api.decref(key_b);
            api.decref(py_a);
            api.decref(py_b);

            let coroutine = api
                .run_string("div(a, b)", PY_EVAL_INPUT, globals, globals)
                .expect("should create coroutine");

            let tstate = api.save_thread();
            let future = crate::future::RubyxFuture::from_coroutine(coroutine, api);
            let result = test_future_value(&future);
            drop(future);
            api.restore_thread(tstate);

            assert!(result.is_err(), "division by zero should propagate");

            api.decref(coroutine);
            api.decref(globals);
        });
    }

    // ========== GIL safety regression tests ==========
    // These tests ensure GIL is always released on error paths.
    // A leaked GIL would deadlock subsequent tests (serial execution).

    #[test]
    #[serial]
    fn test_eval_with_globals_releases_gil_on_python_error() {
        with_ruby_python(|ruby, api| {
            let hash = magnus::RHash::new();
            hash.aset(ruby.sym_new("x"), 1_i64.into_value_with(ruby))
                .unwrap();

            // This should fail (NameError: 'undefined_var') but NOT leak the GIL
            let result =
                crate::eval::rubyx_eval_with_globals("x + undefined_var".to_string(), hash);
            assert!(result.is_err(), "should fail for undefined variable");

            // Prove GIL is released: we can acquire it again without deadlocking
            let gil = api.ensure_gil();
            let check = api.run_string(
                "1 + 1",
                EVAL_INPUT,
                test_make_globals(api),
                test_make_globals(api),
            );
            assert!(check.is_ok());
            api.decref(check.unwrap());
            api.release_gil(gil);
        });
    }

    #[test]
    #[serial]
    fn test_eval_with_globals_releases_gil_on_syntax_error() {
        with_ruby_python(|ruby, api| {
            let hash = magnus::RHash::new();
            hash.aset(ruby.sym_new("x"), 1_i64.into_value_with(ruby))
                .unwrap();

            let result = crate::eval::rubyx_eval_with_globals("def".to_string(), hash);
            assert!(result.is_err(), "should fail on syntax error");

            // GIL should be released — acquire again to verify
            let gil = api.ensure_gil();
            let check = api.run_string(
                "2 + 2",
                EVAL_INPUT,
                test_make_globals(api),
                test_make_globals(api),
            );
            assert!(check.is_ok());
            api.decref(check.unwrap());
            api.release_gil(gil);
        });
    }

    #[test]
    #[serial]
    fn test_await_with_globals_releases_gil_on_error() {
        with_ruby_python(|ruby, api| {
            let hash = magnus::RHash::new();
            hash.aset(ruby.sym_new("x"), 1_i64.into_value_with(ruby))
                .unwrap();

            // Invalid code — should error but release GIL
            let result =
                crate::eval::rubyx_await_with_globals("undefined_coroutine()".to_string(), hash);
            assert!(result.is_err());

            // Verify GIL is free
            let gil = api.ensure_gil();
            let check = api.run_string(
                "3 + 3",
                EVAL_INPUT,
                test_make_globals(api),
                test_make_globals(api),
            );
            assert!(check.is_ok());
            api.decref(check.unwrap());
            api.release_gil(gil);
        });
    }

    #[test]
    #[serial]
    fn test_async_await_with_globals_releases_gil_on_error() {
        with_ruby_python(|ruby, api| {
            let hash = magnus::RHash::new();
            hash.aset(ruby.sym_new("x"), 1_i64.into_value_with(ruby))
                .unwrap();

            // Invalid code — should error but release GIL
            let result =
                crate::eval::rubyx_async_await_with_globals("undefined_async()".to_string(), hash);
            assert!(result.is_err());

            // Verify GIL is free
            let gil = api.ensure_gil();
            let check = api.run_string(
                "4 + 4",
                EVAL_INPUT,
                test_make_globals(api),
                test_make_globals(api),
            );
            assert!(check.is_ok());
            api.decref(check.unwrap());
            api.release_gil(gil);
        });
    }

    #[test]
    #[serial]
    fn test_eval_with_globals_pyguard_drops_before_gil_release() {
        // Regression: PyGuard must drop (decref) BEFORE release_gil.
        // If this deadlocks or segfaults, the ordering is wrong.
        with_ruby_python(|ruby, _api| {
            for _ in 0..50 {
                let hash = magnus::RHash::new();
                hash.aset(ruby.sym_new("val"), 42_i64.into_value_with(ruby))
                    .unwrap();
                let result = crate::eval::rubyx_eval_with_globals("val * 2".to_string(), hash);
                assert!(result.is_ok());
            }
        });
    }

    #[test]
    #[serial]
    fn test_context_eval_with_globals_releases_gil_on_error() {
        with_ruby_python(|ruby, api| {
            let ctx = crate::context::RubyxContext::new().expect("context should create");

            let hash = magnus::RHash::new();
            hash.aset(ruby.sym_new("x"), 1_i64.into_value_with(ruby))
                .unwrap();

            let result = ctx.eval_with_globals("x + missing".to_string(), hash);
            assert!(result.is_err());

            // Verify GIL is free
            let gil = api.ensure_gil();
            let check = api.run_string(
                "5 + 5",
                EVAL_INPUT,
                test_make_globals(api),
                test_make_globals(api),
            );
            assert!(check.is_ok());
            api.decref(check.unwrap());
            api.release_gil(gil);
        });
    }

    #[test]
    #[serial]
    fn test_context_await_with_globals_releases_gil_on_error() {
        with_ruby_python(|ruby, api| {
            let ctx = crate::context::RubyxContext::new().expect("context should create");

            let hash = magnus::RHash::new();
            hash.aset(ruby.sym_new("n"), 1_i64.into_value_with(ruby))
                .unwrap();

            let result = ctx.await_eval_with_globals("no_such_coro()".to_string(), hash);
            assert!(result.is_err());

            // Verify GIL is free
            let gil = api.ensure_gil();
            let check = api.run_string(
                "6 + 6",
                EVAL_INPUT,
                test_make_globals(api),
                test_make_globals(api),
            );
            assert!(check.is_ok());
            api.decref(check.unwrap());
            api.release_gil(gil);
        });
    }

    #[test]
    #[serial]
    fn test_context_async_await_with_globals_releases_gil_on_error() {
        with_ruby_python(|ruby, api| {
            let ctx = crate::context::RubyxContext::new().expect("context should create");

            let hash = magnus::RHash::new();
            hash.aset(ruby.sym_new("n"), 1_i64.into_value_with(ruby))
                .unwrap();

            let result = ctx.async_await_eval_with_globals("no_such_coro()".to_string(), hash);
            assert!(result.is_err());

            // Verify GIL is free
            let gil = api.ensure_gil();
            let check = api.run_string(
                "7 + 7",
                EVAL_INPUT,
                test_make_globals(api),
                test_make_globals(api),
            );
            assert!(check.is_ok());
            api.decref(check.unwrap());
            api.release_gil(gil);
        });
    }

    #[test]
    #[serial]
    fn test_eval_with_globals_error_maps_to_rubyx_class() {
        // Regression: extract_exception was consumed on syntax check,
        // then re-fetched (returning None) → fell back to RuntimeError.
        with_ruby_python(|ruby, _api| {
            let hash = magnus::RHash::new();
            hash.aset(
                ruby.sym_new("d"),
                magnus::RHash::new().into_value_with(ruby),
            )
            .unwrap();

            // Python KeyError should map to Rubyx::KeyError, not RuntimeError
            let result = crate::eval::rubyx_eval_with_globals("d['missing']".to_string(), hash);
            assert!(result.is_err());
            let err_msg = format!("{}", result.unwrap_err());
            assert!(
                err_msg.contains("KeyError"),
                "Expected KeyError in message, got: {}",
                err_msg
            );
        });
    }

    // ========== error class mapping tests ==========

    #[test]
    #[serial]
    fn test_error_mapping_key_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("{}['missing']", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null());
        let exc = PythonApi::extract_exception(api);
        assert!(exc.is_some());
        if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
            assert_eq!(kind, "KeyError");
        } else {
            panic!("Expected KeyError, got: {:?}", exc);
        }
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_error_mapping_index_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("[][5]", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null());
        let exc = PythonApi::extract_exception(api);
        if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
            assert_eq!(kind, "IndexError");
        } else {
            panic!("Expected IndexError, got: {:?}", exc);
        }
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_error_mapping_value_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("int('bad')", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null());
        let exc = PythonApi::extract_exception(api);
        if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
            assert_eq!(kind, "ValueError");
        } else {
            panic!("Expected ValueError, got: {:?}", exc);
        }
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_error_mapping_type_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("1 + 'a'", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null());
        let exc = PythonApi::extract_exception(api);
        if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
            assert_eq!(kind, "TypeError");
        } else {
            panic!("Expected TypeError, got: {:?}", exc);
        }
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_error_mapping_attribute_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("(1).nonexistent", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null());
        let exc = PythonApi::extract_exception(api);
        if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
            assert_eq!(kind, "AttributeError");
        } else {
            panic!("Expected AttributeError, got: {:?}", exc);
        }
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_error_mapping_zero_division_falls_to_python_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = test_make_globals(api);

        let result = api.run_string("1/0", EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null());
        let exc = PythonApi::extract_exception(api);
        if let Some(crate::exception::PythonException::Exception { kind, .. }) = &exc {
            assert_eq!(kind, "ZeroDivisionError");
        } else {
            panic!("Expected ZeroDivisionError, got: {:?}", exc);
        }
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_rubyx_exception_class_maps_known_kinds() {
        with_ruby_python(|ruby, _api| {
            use crate::ruby_helpers::rubyx_exception_class;
            use magnus::{Class, Module};

            // Define Rubyx error classes (normally done by error.rb at gem load time).
            // define_class needs RClass, so we eval to create exception subclasses.
            ruby.eval::<magnus::Value>(
                "
                module Rubyx
                  class Error < StandardError; end
                  class PythonError < Error; end
                  class KeyError < Error; end
                  class IndexError < Error; end
                  class ValueError < Error; end
                  class TypeError < Error; end
                  class AttributeError < Error; end
                  class ImportError < PythonError; end
                end
            ",
            )
            .expect("should define error classes");

            let key_err = rubyx_exception_class("KeyError");
            let idx_err = rubyx_exception_class("IndexError");
            let val_err = rubyx_exception_class("ValueError");
            let typ_err = rubyx_exception_class("TypeError");
            let attr_err = rubyx_exception_class("AttributeError");
            let imp_err = rubyx_exception_class("ImportError");
            let mnf_err = rubyx_exception_class("ModuleNotFoundError");
            let unknown = rubyx_exception_class("ZeroDivisionError");

            let class_name =
                |c: magnus::ExceptionClass| -> String { unsafe { c.name().to_string() } };

            assert_eq!(class_name(key_err), "Rubyx::KeyError");
            assert_eq!(class_name(idx_err), "Rubyx::IndexError");
            assert_eq!(class_name(val_err), "Rubyx::ValueError");
            assert_eq!(class_name(typ_err), "Rubyx::TypeError");
            assert_eq!(class_name(attr_err), "Rubyx::AttributeError");
            assert_eq!(class_name(imp_err), "Rubyx::ImportError");
            assert_eq!(class_name(mnf_err), "Rubyx::ImportError");
            assert_eq!(class_name(unknown), "Rubyx::PythonError");
        });
    }

    // ========== respond_to_missing? tests ==========

    #[test]
    #[serial]
    fn test_respond_to_missing_via_ruby() {
        with_ruby_python(|ruby, api| {
            let os = api.import_module("os").expect("os should import");
            let wrapper = RubyxObject::new(os, api).unwrap();

            // Test with symbol (Ruby convention)
            let args = vec!["path".into_value_with(ruby)];
            assert!(
                wrapper.respond_to_missing(&args).unwrap(),
                "os.path should exist"
            );

            // Test nonexistent
            let args = vec!["xyz_not_real".into_value_with(ruby)];
            assert!(
                !wrapper.respond_to_missing(&args).unwrap(),
                "nonexistent should be false"
            );

            drop(wrapper);
            api.decref(os);
        });
    }

    #[test]
    #[serial]
    fn test_implicit_conversion_guards_dont_delegate() {
        with_ruby_python(|ruby, api| {
            let py_list = unsafe { (api.py_list_new)(0) };
            let wrapper = RubyxObject::new(py_list, api).unwrap();

            // All of these should raise NoMethodError, not delegate to Python
            for method in &[
                "to_ary", "to_str", "to_hash", "to_int", "to_float", "to_io", "to_proc",
            ] {
                let args = vec![(*method).into_value_with(ruby)];
                let result = wrapper.method_missing(&args);
                assert!(
                    result.is_err(),
                    "{} should be guarded, not delegated to Python",
                    method
                );
            }

            drop(wrapper);
            api.decref(py_list);
        });
    }

    #[test]
    #[serial]
    fn test_respond_to_missing_on_module() {
        with_ruby_python(|ruby, api| {
            let json = api.import_module("json").expect("json should import");
            let wrapper = RubyxObject::new(json, api).unwrap();

            // json.loads and json.dumps should exist
            assert!(wrapper
                .respond_to_missing(&["loads".into_value_with(ruby)])
                .unwrap());
            assert!(wrapper
                .respond_to_missing(&["dumps".into_value_with(ruby)])
                .unwrap());

            // json.nonexistent should not
            assert!(!wrapper
                .respond_to_missing(&["nonexistent".into_value_with(ruby)])
                .unwrap());

            drop(wrapper);
            api.decref(json);
        });
    }

    // ========== getitem / setitem / delitem integration ==========

    #[test]
    #[serial]
    fn test_getitem_setitem_roundtrip() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);
            let py_dict = api
                .run_string("{'x': 10}", 258, globals, globals)
                .expect("should create dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            // Read existing
            let key: magnus::Value = "x".into_value_with(ruby);
            let result = wrapper.getitem(key).expect("should read 'x'");
            let obj = magnus::typed_data::Obj::<RubyxObject>::try_convert(result).unwrap();
            assert_eq!(api.long_to_i64(obj.as_ptr()), 10);

            // Write new
            let new_key: magnus::Value = "y".into_value_with(ruby);
            let new_val: magnus::Value = 20_i64.into_value_with(ruby);
            wrapper.setitem(new_key, new_val).expect("should set 'y'");

            // Read back
            let check: magnus::Value = "y".into_value_with(ruby);
            let result2 = wrapper.getitem(check).expect("should read 'y'");
            let obj2 = magnus::typed_data::Obj::<RubyxObject>::try_convert(result2).unwrap();
            assert_eq!(api.long_to_i64(obj2.as_ptr()), 20);

            drop(wrapper);
            api.decref(py_dict);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_delitem_then_getitem_fails() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);
            let py_dict = api
                .run_string("{'remove_me': 999}", 258, globals, globals)
                .expect("should create dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            let key: magnus::Value = "remove_me".into_value_with(ruby);
            wrapper.delitem(key).expect("should delete key");

            let check: magnus::Value = "remove_me".into_value_with(ruby);
            assert!(
                wrapper.getitem(check).is_err(),
                "deleted key should not be found"
            );

            drop(wrapper);
            api.decref(py_dict);
            api.decref(globals);
        });
    }

    #[test]
    #[serial]
    fn test_getitem_list_integration() {
        with_ruby_python(|ruby, api| {
            let globals = make_globals(api);
            let py_list = api
                .run_string("['a', 'b', 'c']", 258, globals, globals)
                .expect("should create list");
            let wrapper = RubyxObject::new(py_list, api).unwrap();

            for (i, expected) in ["a", "b", "c"].iter().enumerate() {
                let key: magnus::Value = (i as i64).into_value_with(ruby);
                let result = wrapper.getitem(key).expect("should read index");
                let obj = magnus::typed_data::Obj::<RubyxObject>::try_convert(result).unwrap();
                assert_eq!(
                    api.string_to_string(obj.as_ptr()),
                    Some(expected.to_string())
                );
            }

            drop(wrapper);
            api.decref(py_list);
            api.decref(globals);
        });
    }
}
