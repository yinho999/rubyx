use crate::convert::ToPython;
use crate::python_api::PythonApi;
use crate::python_ffi::PyObject;
use crate::python_guard::PyGuard;
use crate::ruby_helpers;
use crate::stream::SendableValue;
use magnus::r_hash::ForEach;
use magnus::typed_data::Obj;
use magnus::value::ReprValue;
use magnus::{Class, IntoValue, RHash, Ruby, Symbol, TryConvert, Value};
use std::ffi::CString;

const RUBY_IMPLICIT_CONVERSIONS: &[&str] = &[
    "to_ary",
    "to_str",
    "to_hash",
    "to_int",
    "to_float",
    "to_io",
    "to_proc",
    "to_path",
    "to_regexp",
];

pub(crate) fn python_to_sendable(
    py_val: *mut PyObject,
    api: &PythonApi,
) -> Result<SendableValue, String> {
    // Nil
    if py_val == api.py_none {
        return Ok(SendableValue::Nil);
    }
    // Bool must be checked before long, because Python bool is a subclass of int
    if api.is_bool(py_val) {
        return Ok(SendableValue::Bool(py_val == api.py_true));
    }
    if api.is_long(py_val) {
        let val = api.long_to_i64(py_val);
        return Ok(SendableValue::Integer(val));
    }
    if api.is_float(py_val) {
        let val = api.float_to_f64(py_val);
        return Ok(SendableValue::Float(val));
    }
    if api.is_string(py_val) {
        let Some(val) = api.string_to_string(py_val) else {
            if api.has_error() {
                api.clear_error();
            }
            return Err("Cannot decode Python string as UTF-8".to_string());
        };
        return Ok(SendableValue::Str(val));
    }
    if api.tuple_check(py_val) {
        let len = api.tuple_size(py_val);
        let mut items = Vec::with_capacity(len as usize);
        for i in 0..len {
            let item = api.tuple_get_item(py_val, i);
            items.push(python_to_sendable(item, api)?);
        }
        return Ok(SendableValue::List(items));
    }

    if api.is_set(py_val) || api.is_frozen_set(py_val) {
        let len = api.set_size(py_val);
        let mut items = Vec::with_capacity(len as usize);
        let iter = api.object_get_iter(py_val);
        loop {
            let item = api.iter_next(iter);
            if item.is_null() {
                break;
            }
            let result = python_to_sendable(item, api);
            api.decref(item);
            match result {
                Ok(val) => items.push(val),
                Err(e) => {
                    api.decref(iter);
                    return Err(e);
                }
            }
        }
        if api.has_error() {
            api.clear_error();
        }
        api.decref(iter);
        return Ok(SendableValue::Set(items));
    }

    if api.list_check(py_val) {
        let len = api.list_size(py_val);
        let mut items = Vec::with_capacity(len as usize);
        for i in 0..len {
            let item = api.list_get_item(py_val, i);
            items.push(python_to_sendable(item, api)?);
        }
        return Ok(SendableValue::List(items));
    }

    if api.dict_check(py_val) {
        let len = api.dict_size(py_val);
        let mut items = Vec::with_capacity(len);
        let mut start = 0;
        let mut key = std::ptr::null_mut();
        let mut value = std::ptr::null_mut();
        while api.dict_next(py_val, &mut start, &mut key, &mut value) {
            let send_key = python_to_sendable(key, api)?;
            let send_value = python_to_sendable(value, api)?;
            items.push((send_key, send_value));
        }
        return Ok(SendableValue::Dict(items));
    }

    if py_val == api.py_true {
        return Ok(SendableValue::Bool(true));
    }
    if py_val == api.py_false {
        return Ok(SendableValue::Bool(false));
    }
    let has_dict = {
        let name = std::ffi::CString::new("__dict__").unwrap();
        api.object_has_attr_string(py_val, name.as_ptr()) != 0
    };
    if has_dict || api.callable_check(py_val) != 0 {
        api.incref(py_val);
        return Ok(SendableValue::PyObjectRef(py_val as usize));
    }
    Err("Cannot convert Python value to Ruby".to_string())
}
pub(crate) fn ruby_to_python(
    value: Value,
    api: &PythonApi,
) -> Result<*mut PyObject, magnus::Error> {
    let ruby = Ruby::get().map_err(|e| {
        magnus::Error::new(
            ruby_helpers::runtime_error(),
            format!("Ruby VM handle unavailable: {e}"),
        )
    })?;
    if value.is_nil() {
        api.incref(api.py_none);
        return Ok(api.py_none);
    }
    if value.is_kind_of(ruby.class_true_class()) {
        api.incref(api.py_true);
        return Ok(api.py_true);
    }
    if value.is_kind_of(ruby.class_false_class()) {
        api.incref(api.py_false);
        return Ok(api.py_false);
    }
    if value.is_kind_of(ruby.class_integer()) {
        let val = i64::try_convert(value)?;
        return val
            .to_python(api)
            .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e.to_string()));
    }
    if value.is_kind_of(ruby.class_float()) {
        let val = f64::try_convert(value)?;
        return val
            .to_python(api)
            .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e.to_string()));
    }
    if value.is_kind_of(ruby.class_symbol()) {
        let sym = Symbol::try_convert(value)?;
        let name = sym.name().map_err(|e| {
            magnus::Error::new(
                ruby_helpers::runtime_error(),
                format!("Symbol name error: {e}"),
            )
        })?;
        let py_str = api.string_from_str(name.as_ref());
        if py_str.is_null() {
            return Err(magnus::Error::new(
                ruby_helpers::runtime_error(),
                "Failed to create Python string from Symbol",
            ));
        }
        return Ok(py_str);
    }
    if value.is_kind_of(ruby.class_string()) {
        let val = String::try_convert(value)?;
        return val
            .to_python(api)
            .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e.to_string()));
    }
    if value.is_kind_of(ruby.class_array()) {
        let arr = magnus::RArray::try_convert(value)?;
        let len = arr.len();
        let py_list = api.list_new(len as isize);
        if py_list.is_null() {
            return Err(magnus::Error::new(
                ruby_helpers::runtime_error(),
                "Failed to create Python list",
            ));
        }
        for (i, item) in arr.into_iter().enumerate() {
            let py_item = ruby_to_python(item, api).inspect_err(|_e| {
                api.decref(py_list);
            })?;
            let result = api.list_set_item(py_list, i as isize, py_item);
            if result != 0 {
                api.decref(py_item);
                api.decref(py_list);
                return Err(magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    "Failed to set Python list item",
                ));
            }
        }
        return Ok(py_list);
    }
    if value.is_kind_of(ruby.class_hash()) {
        let hash = RHash::try_convert(value)?;
        let dict = api.dict_new();
        if dict.is_null() {
            return Err(magnus::Error::new(
                ruby_helpers::runtime_error(),
                "Failed to create Python dict",
            ));
        }
        let mut err: Option<magnus::Error> = None;
        hash.foreach(|k: Value, v: Value| {
            let py_key = match ruby_to_python(k, api) {
                Ok(k) => k,
                Err(e) => {
                    err = Some(e);
                    return Ok(ForEach::Stop);
                }
            };
            let py_val = match ruby_to_python(v, api) {
                Ok(v) => v,
                Err(e) => {
                    api.decref(py_key);
                    err = Some(e);
                    return Ok(ForEach::Stop);
                }
            };
            let result = api.dict_set_item(dict, py_key, py_val);
            api.decref(py_key);
            api.decref(py_val);
            if result == -1 {
                err = Some(magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    "Failed to set Python dict item",
                ));
                return Ok(ForEach::Stop);
            }
            Ok(ForEach::Continue)
        })?;
        if let Some(e) = err {
            api.decref(dict);
            return Err(e);
        }
        return Ok(dict);
    }
    // Already wrapped Python object
    if let Ok(obj) = Obj::<RubyxObject>::try_convert(value) {
        api.incref(obj.as_ptr());
        return Ok(obj.as_ptr());
    }
    Err(magnus::Error::new(
        ruby_helpers::type_error(),
        format!("Cannot convert {} to Python object", unsafe {
            value.class().name()
        }),
    ))
}

/// A Ruby object that wraps a Python object.
/// Handles cross-language GC coordination.
#[magnus::wrap(class = "RubyxObject", mark, free_immediately, size)]
pub struct RubyxObject {
    py_obj: *mut PyObject,
    api: &'static PythonApi,
}
unsafe impl Send for RubyxObject {}
unsafe impl Sync for RubyxObject {}
impl RubyxObject {
    /// Create a new wrapper, incrementing the Python object's reference count.
    pub fn new(py_obj: *mut PyObject, api: &'static PythonApi) -> Option<Self> {
        if py_obj.is_null() {
            return None;
        }
        if !api.is_initialized() {
            return None;
        }
        // ensure_gil is reentrant — safe even if caller already holds GIL
        let gil = api.ensure_gil();
        // Increase refcount
        api.incref(py_obj);
        api.release_gil(gil);
        Some(RubyxObject { py_obj, api })
    }

    pub fn as_ptr(&self) -> *mut PyObject {
        self.py_obj
    }

    /// This method provides a dynamic dispatch mechanism to resolve and call methods on Python objects
    /// in a Ruby environment using the `magnus` bridge and internal Python C API bindings.
    ///
    /// The `method_missing` function is the Ruby equivalent of handling undefined method calls (e.g., `obj.foo`)
    /// on a Ruby object, but it utilizes Python interop to dynamically retrieve, set, or invoke Python attributes
    /// and methods, depending on the method call's context.
    ///
    /// # Arguments
    /// - `&self`: Reference to the current object which interacts with a Python object.
    /// - `args`: A slice of `magnus::Value` that represents Ruby arguments. This typically includes:
    ///   * The name of the method being called as a Symbol/String.
    ///   * Any additional arguments for a method call or value in the case of setters.
    /// # Returns
    /// - `Result<magnus::Value, magnus::Error>`:
    ///   * On success, returns a `magnus::Value` object that represents the result of the Python interaction,
    ///     whether it's an attribute access, setter operation, or method call.
    ///   * On failure, returns a `magnus::Error` containing details about the failure reason.
    ///
    /// # Error Handling
    /// - Raises `magnus::Error` for invalid invocation patterns:
    ///   * If `args` is empty.
    ///   * If the method name is not a valid String or Symbol.
    ///   * If the method attempts a setter operation with an incorrect number of arguments.
    /// - Handles Ruby and Python exceptions during API interop by translating them into appropriate `magnus::Error`s.
    /// # Examples
    /// ```ruby
    /// obj.foo         # Triggers a Python attribute getter
    /// obj.foo(1, 2)   # Triggers a Python method call with positional arguments
    /// obj.foo = value # Triggers a Python attribute setter
    /// ```
    ///
    /// ## Ruby Code to `args` Slice Mapping
    ///
    /// The `args` parameter is a flat slice where `args[0]` is always the method name
    /// (Symbol or String), and the remaining elements are the call arguments. Ruby's
    /// `method_missing(*args)` (declared with arity `-1` in Magnus) packs everything
    /// into this single slice.
    ///
    /// | Ruby Code                        | `args` Slice                                       | Dispatch Path     |
    /// |----------------------------------|-----------------------------------------------------|-------------------|
    /// | `obj.foo`                        | `[:foo]`                                            | Getter            |
    /// | `obj.foo = 42`                   | `[:"foo=", 42]`                                    | Setter            |
    /// | `obj.foo(1, 2)`                  | `[:foo, 1, 2]`                                     | Call (positional)  |
    /// | `obj.foo(a, k: v)`               | `[:foo, a, {k: v}]`                                | Call (pos + kwargs)|
    /// | `obj.dumps(data, indent: 2)`     | `[:dumps, data, {indent: 2}]`                      | Call (pos + kwargs)|
    ///
    /// ### Getter (`args.len() == 1`, no `=` suffix)
    /// ```ruby
    /// obj.foo         # args = [:foo]
    /// ```
    /// Resolves via `PyObject_GetAttrString`. If the attribute is non-callable, it is
    /// returned directly as a wrapped `RubyxObject`.
    ///
    /// ### Setter (`args[0]` ends with `=`, `args.len() == 2`)
    /// ```ruby
    /// obj.foo = value # args = [:"foo=", value]
    /// ```
    /// The trailing `=` is stripped to get the attribute name, then
    /// `PyObject_SetAttrString` is called with the converted Python value.
    ///
    /// ### Callable (`args.len() > 1`, or attribute is callable)
    /// ```ruby
    /// obj.foo(1, 2)              # args = [:foo, 1, 2]          → positional only
    /// obj.foo(1, key: "val")     # args = [:foo, 1, {key: "val"}] → positional + kwargs
    /// ```
    /// Positional arguments are `args[1..]` (excluding a trailing Hash). If the last
    /// element in `args[1..]` is a Ruby `Hash`, it is split off and converted to a
    /// Python kwargs dict. A Python tuple is built from the positional arguments, and
    /// the call is dispatched via `PyObject_Call(callable, args_tuple, kwargs_dict)`.
    ///
    /// # Limitations
    /// - Currently restricted to single inheritance where the missing Ruby method maps directly to a single Python
    ///   object interaction.
    /// - Keyword arguments (kwargs) are only supported if the last Ruby argument is a hash that can be converted to a Python dict.
    pub fn method_missing(&self, args: &[magnus::Value]) -> Result<magnus::Value, magnus::Error> {
        let api = self.api;
        let gil = api.ensure_gil();

        // Get python attribute if exist
        let result = (|| -> Result<Value, magnus::Error> {
            if args.is_empty() {
                return Err(magnus::Error::new(
                    ruby_helpers::arg_error(),
                    "No method name given",
                ));
            }
            let ruby = Ruby::get().map_err(|e| {
                magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    format!("Ruby VM handle unavailable: {e}"),
                )
            })?;
            let method_name = if let Ok(s) = String::try_convert(args[0]) {
                s
            } else if let Ok(sym) = Symbol::try_convert(args[0]) {
                sym.name()?.to_string()
            } else {
                return Err(magnus::Error::new(
                    ruby_helpers::type_error(),
                    "method_missing expects Symbol/String method name",
                ));
            };

            if RUBY_IMPLICIT_CONVERSIONS.contains(&method_name.as_str()) {
                return Err(magnus::Error::new(
                    ruby_helpers::no_method_error(),
                    format!("undefined method '{}' for RubyxObject", method_name),
                ));
            }

            // Setter - `obj.foo = value`
            if method_name.ends_with("=") {
                if args.len() != 2 {
                    return Err(magnus::Error::new(
                        ruby_helpers::arg_error(),
                        "Setter required exactly one value",
                    ));
                }
                let attr_name = &method_name[..method_name.len() - 1];
                let py_value = ruby_to_python(args[1], api)?;
                let rc = api.object_set_attr_string(self.py_obj, attr_name, py_value);
                api.decref(py_value); // set_attr_string does not steal reference
                if rc != 0 {
                    if let Some(py_err) = PythonApi::extract_exception(api) {
                        return Err(magnus::Error::from(py_err));
                    }
                    return Err(magnus::Error::new(
                        ruby_helpers::runtime_error(),
                        "Failed to set Python attribute",
                    ));
                }
                return Ok(args[1]);
            }
            // Getter - `obj.foo`
            let python_attr = api.object_get_attr_string(self.py_obj, &method_name);
            if python_attr.is_null() {
                api.clear_error();
                return Err(magnus::Error::new(
                    ruby_helpers::exception(),
                    format!("undefined method `{method_name}` for a Python object"),
                ));
            }
            let py_attr_guard = PyGuard::new(python_attr, api).ok_or_else(|| {
                magnus::Error::new(ruby_helpers::runtime_error(), "Null Python attribute")
            })?;

            // Attribute read path (non-callable + no args) - `obj.foo`
            if api.callable_check(py_attr_guard.ptr()) == 0 && args.len() == 1 {
                let wrapper = RubyxObject::new(py_attr_guard.ptr(), api).ok_or_else(|| {
                    magnus::Error::new(
                        ruby_helpers::runtime_error(),
                        "Failed to wrap Python attribute",
                    )
                })?;
                return Ok(wrapper.into_value_with(&ruby));
            }
            // Call path - `obj.foo(args)`
            let call_args = &args[1..];

            // Optional kwargs: last arg hash
            let (positional, kwargs) = if let Some(last) = call_args.last() {
                if last.is_kind_of(ruby.class_hash()) {
                    (
                        &call_args[..call_args.len() - 1],
                        Some(RHash::try_convert(*last)?),
                    )
                } else {
                    (call_args, None)
                }
            } else {
                (call_args, None)
            };

            // Args Tuple for args
            let py_args = api.tuple_new(positional.len() as isize);
            if py_args.is_null() {
                return Err(magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    "Failed to allocate Python args tuple",
                ));
            }
            let py_args_guard = PyGuard::new(py_args, api).ok_or_else(|| {
                magnus::Error::new(ruby_helpers::runtime_error(), "Null Python args tuple")
            })?;
            for (i, arg) in positional.iter().enumerate() {
                let py_arg = ruby_to_python(*arg, api)?;
                // tuple_set_item steals reference on success
                if api.tuple_set_item(py_args_guard.ptr(), i as isize, py_arg) != 0 {
                    api.decref(py_arg); // only decref on failure
                    if let Some(py_err) = PythonApi::extract_exception(api) {
                        return Err(magnus::Error::from(py_err));
                    }
                    return Err(magnus::Error::new(
                        ruby_helpers::runtime_error(),
                        "Failed to set tuple argument",
                    ));
                }
            }
            // Kwargs Dict for kwargs
            let py_kwargs_guard = if let Some(hash) = kwargs {
                // Convert kwargs to Python dict
                let dict = api.dict_new();
                if dict.is_null() {
                    return Err(magnus::Error::new(
                        ruby_helpers::runtime_error(),
                        "Failed to allocate kwargs dict",
                    ));
                }
                let guard = PyGuard::new(dict, api).ok_or_else(|| {
                    magnus::Error::new(ruby_helpers::runtime_error(), "Null kwargs dict")
                })?;
                // Save the key and value to python dict
                hash.foreach(|k: Value, v: Value| {
                    let key = if let Ok(s) = String::try_convert(k) {
                        s
                    } else if let Ok(sym) = Symbol::try_convert(k) {
                        sym.name()?.to_string()
                    } else {
                        return Err(magnus::Error::new(
                            ruby_helpers::type_error(),
                            "kwargs keys must be String or Symbol",
                        ));
                    };
                    let py_key = key.to_python(api).map_err(|e| {
                        magnus::Error::new(ruby_helpers::runtime_error(), format!("{e:?}"))
                    })?;
                    let py_val = ruby_to_python(v, api)?;
                    let rc = api.dict_set_item(guard.ptr(), py_key, py_val);
                    // dict_set_item does not steal
                    api.decref(py_key);
                    api.decref(py_val);
                    if rc != 0 {
                        if let Some(py_err) = PythonApi::extract_exception(api) {
                            return Err(magnus::Error::from(py_err));
                        }
                        return Err(magnus::Error::new(
                            ruby_helpers::runtime_error(),
                            "Failed to set kwargs item",
                        ));
                    }
                    Ok(ForEach::Continue)
                })?;
                Some(guard)
            } else {
                None
            };
            let py_kwargs_ptr = py_kwargs_guard
                .as_ref()
                .map_or(std::ptr::null_mut(), |g| g.ptr());
            let py_result =
                api.object_call(py_attr_guard.ptr(), py_args_guard.ptr(), py_kwargs_ptr);
            if py_result.is_null() {
                if let Some(py_err) = PythonApi::extract_exception(api) {
                    return Err(magnus::Error::from(py_err));
                }
                return Err(magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    "Python call failed",
                ));
            }
            let py_result_guard = PyGuard::new(py_result, api).ok_or_else(|| {
                magnus::Error::new(ruby_helpers::runtime_error(), "Null Python result")
            })?;
            let wrapper = RubyxObject::new(py_result_guard.ptr(), api).ok_or_else(|| {
                magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    "Failed to wrap a Python result",
                )
            })?;
            Ok(wrapper.into_value_with(&ruby))
        })();
        api.release_gil(gil);
        result
    }

    pub fn respond_to_missing(&self, args: &[magnus::Value]) -> Result<bool, magnus::Error> {
        if args.is_empty() {
            return Err(magnus::Error::new(
                ruby_helpers::arg_error(),
                "No method name given",
            ));
        }
        let name = if let Ok(s) = String::try_convert(args[0]) {
            s
        } else if let Ok(sym) = Symbol::try_convert(args[0]) {
            sym.name()?.to_string()
        } else {
            return Err(magnus::Error::new(
                ruby_helpers::type_error(),
                "method_missing expects Symbol/String method name",
            ));
        };

        let api = self.api;
        let gil = api.ensure_gil();
        let c_name = CString::new(name.as_str())
            .map_err(|_| magnus::Error::new(ruby_helpers::arg_error(), "Invalid method name"))?;
        let result = api.object_has_attr_string(self.as_ptr(), c_name.as_ptr()) != 0;
        api.release_gil(gil);
        Ok(result)
    }

    pub fn to_s(&self) -> Result<String, magnus::Error> {
        let api = self.api;
        let gil = api.ensure_gil();
        let py_str = api.object_str(self.as_ptr());
        let result = if py_str.is_null() {
            api.clear_error();
            format!("#<RubyxObject:{:p}>", self.as_ptr())
        } else {
            let s = api.string_to_string(py_str).unwrap_or_default();
            api.decref(py_str);
            s
        };

        api.release_gil(gil);
        Ok(result)
    }

    pub fn inspect(&self) -> Result<String, magnus::Error> {
        let api = self.api;
        let gil = api.ensure_gil();
        let result = api.object_repr(self.as_ptr());

        api.release_gil(gil);
        Ok(result)
    }

    pub fn to_ruby(&self) -> Result<magnus::Value, magnus::Error> {
        let api = self.api;
        let gil = api.ensure_gil();

        let sendable = python_to_sendable(self.as_ptr(), api)
            .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e));

        api.release_gil(gil);

        sendable?.try_into()
    }

    pub fn getitem(&self, key: Value) -> Result<Value, magnus::Error> {
        let api = self.api;
        let gil = api.ensure_gil();
        let ruby = Ruby::get()
            .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e.to_string()))?;

        let py_key = ruby_to_python(key, api)?;
        let result = api.object_get_item(self.as_ptr(), py_key);
        api.decref(py_key);

        if result.is_null() {
            let err = if let Some(exc) = PythonApi::extract_exception(api) {
                magnus::Error::from(exc)
            } else {
                magnus::Error::new(ruby_helpers::runtime_error(), "KeyError or IndexError")
            };
            api.release_gil(gil);
            return Err(err);
        }

        let wrapper = RubyxObject::new(result, api).ok_or_else(|| {
            magnus::Error::new(ruby_helpers::runtime_error(), "Failed to wrap result")
        })?;
        api.release_gil(gil);
        Ok(wrapper.into_value_with(&ruby))
    }

    pub fn setitem(&self, key: Value, value: Value) -> Result<Value, magnus::Error> {
        let api = self.api;
        let gil = api.ensure_gil();

        let py_key = ruby_to_python(key, api)?;
        let py_val = ruby_to_python(value, api)?;
        let result = api.object_set_item(self.as_ptr(), py_key, py_val);
        api.decref(py_key);
        api.decref(py_val);

        if result == -1 {
            let err = if let Some(exc) = PythonApi::extract_exception(api) {
                magnus::Error::from(exc)
            } else {
                magnus::Error::new(ruby_helpers::runtime_error(), "Failed to set item")
            };
            api.release_gil(gil);
            return Err(err);
        }

        api.release_gil(gil);
        Ok(value)
    }

    pub fn delitem(&self, key: Value) -> Result<Value, magnus::Error> {
        let api = self.api;
        let gil = api.ensure_gil();
        let ruby = Ruby::get()
            .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e.to_string()))?;

        let py_key = ruby_to_python(key, api)?;
        let result = api.object_del_item(self.as_ptr(), py_key);
        api.decref(py_key);

        if result == -1 {
            let err = if let Some(exc) = PythonApi::extract_exception(api) {
                magnus::Error::from(exc)
            } else {
                magnus::Error::new(ruby_helpers::runtime_error(), "Failed to delete item")
            };
            api.release_gil(gil);
            return Err(err);
        }

        api.release_gil(gil);
        Ok(ruby.qnil().as_value())
    }

    pub fn each(&self) -> Result<Value, magnus::Error> {
        let ruby = Ruby::get()
            .map_err(|e| magnus::Error::new(ruby_helpers::runtime_error(), e.to_string()))?;

        if !ruby.block_given() {
            let receiver: Value = ruby.current_receiver()?;
            return Ok(receiver.enumeratorize("each", ()).as_value());
        }

        let api = self.api;
        let gil = api.ensure_gil();

        let py_iter = api.object_get_iter(self.as_ptr());
        if py_iter.is_null() {
            api.clear_error();
            api.release_gil(gil);
            return Err(magnus::Error::new(
                ruby_helpers::type_error(),
                "Python object is not iterable",
            ));
        }

        // Use closure to ensure cleanup (decref + release_gil) runs on all paths,
        // including early returns from yield_value (Ruby break) or wrap failures.
        let result = (|| -> Result<(), magnus::Error> {
            loop {
                let item = api.iter_next(py_iter);
                if item.is_null() {
                    if api.has_error() {
                        let exc = PythonApi::extract_exception(api);
                        if let Some(e) = exc {
                            return Err(magnus::Error::from(e));
                        }
                        return Err(magnus::Error::new(
                            ruby_helpers::runtime_error(),
                            "Python iteration error",
                        ));
                    }
                    break;
                }

                let wrapper = RubyxObject::new(item, api).ok_or_else(|| {
                    magnus::Error::new(ruby_helpers::runtime_error(), "Failed to wrap item")
                })?;
                let val = wrapper.into_value_with(&ruby);
                let _: Value = ruby.yield_value(val)?;
            }
            Ok(())
        })();

        // Always cleanup — regardless of success or error
        api.decref(py_iter);
        api.release_gil(gil);

        result?;
        Ok(ruby.qnil().as_value())
    }

    pub fn is_truthy(&self) -> bool {
        let gil = self.api.ensure_gil();
        let result = self.api.object_is_true(self.py_obj);
        self.api.release_gil(gil);
        result
    }

    pub fn is_falsy(&self) -> bool {
        !self.is_truthy()
    }

    pub fn is_callable(&self) -> bool {
        let gil = self.api.ensure_gil();
        let result = self.api.callable_check(self.py_obj) != 0;
        self.api.release_gil(gil);
        result
    }

    pub fn call(&self, args: &[magnus::Value]) -> Result<Value, magnus::Error> {
        let api = self.api;
        let gil = api.ensure_gil();
        let result = (|| -> Result<Value, magnus::Error> {
            if api.callable_check(self.py_obj) == 0 {
                return Err(magnus::Error::new(
                    ruby_helpers::type_error(),
                    "Python object is not callable",
                ));
            }
            let ruby = Ruby::get().map_err(|e| {
                magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    format!("Ruby VM handle unavailable: {e}"),
                )
            })?;

            // Extract positional and keyword arguments
            let (positional, kwargs) = if let Some(last) = args.last() {
                if last.is_kind_of(ruby.class_hash()) {
                    (&args[..args.len() - 1], Some(RHash::try_convert(*last)?))
                } else {
                    (args, None)
                }
            } else {
                (args, None)
            };

            // Args Tuple for args
            let py_args = api.tuple_new(positional.len() as isize);
            if py_args.is_null() {
                return Err(magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    "Failed to allocate Python args tuple",
                ));
            }
            let py_args_guard = PyGuard::new(py_args, api).ok_or_else(|| {
                magnus::Error::new(ruby_helpers::runtime_error(), "Null Python args tuple")
            })?;
            for (i, arg) in positional.iter().enumerate() {
                let py_arg = ruby_to_python(*arg, api)?;
                if api.tuple_set_item(py_args_guard.ptr(), i as isize, py_arg) != 0 {
                    api.decref(py_arg);
                    if let Some(py_err) = PythonApi::extract_exception(api) {
                        return Err(magnus::Error::from(py_err));
                    }
                    return Err(magnus::Error::new(
                        ruby_helpers::runtime_error(),
                        "Failed to set tuple argument",
                    ));
                }
            }

            // kwargs dict
            let py_kwargs_guard = if let Some(hash) = kwargs {
                let dict = api.dict_new();
                if dict.is_null() {
                    return Err(magnus::Error::new(
                        ruby_helpers::runtime_error(),
                        "Failed to allocate kwargs dict",
                    ));
                }
                let guard = PyGuard::new(dict, api).ok_or_else(|| {
                    magnus::Error::new(ruby_helpers::runtime_error(), "Null kwargs dict")
                })?;
                hash.foreach(|k: Value, v: Value| {
                    let key = if let Ok(s) = String::try_convert(k) {
                        s
                    } else if let Ok(sym) = Symbol::try_convert(k) {
                        sym.name()?.to_string()
                    } else {
                        return Err(magnus::Error::new(
                            ruby_helpers::type_error(),
                            "kwargs keys must be String or Symbol",
                        ));
                    };
                    let py_key = key.to_python(api).map_err(|e| {
                        magnus::Error::new(ruby_helpers::runtime_error(), format!("{e:?}"))
                    })?;
                    let py_val = ruby_to_python(v, api)?;
                    let rc = api.dict_set_item(guard.ptr(), py_key, py_val);
                    api.decref(py_key);
                    api.decref(py_val);
                    if rc != 0 {
                        if let Some(py_err) = PythonApi::extract_exception(api) {
                            return Err(magnus::Error::from(py_err));
                        }
                        return Err(magnus::Error::new(
                            ruby_helpers::runtime_error(),
                            "Failed to set kwargs item",
                        ));
                    }
                    Ok(ForEach::Continue)
                })?;
                Some(guard)
            } else {
                None
            };

            let py_kwargs_ptr = py_kwargs_guard
                .as_ref()
                .map_or(std::ptr::null_mut(), |g| g.ptr());
            // call the python callable with args and kwargs
            let py_result = api.object_call(self.py_obj, py_args_guard.ptr(), py_kwargs_ptr);
            if py_result.is_null() {
                if let Some(py_err) = PythonApi::extract_exception(api) {
                    return Err(magnus::Error::from(py_err));
                }
                return Err(magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    "Python call failed",
                ));
            }
            let py_result_guard = PyGuard::new(py_result, api).ok_or_else(|| {
                magnus::Error::new(ruby_helpers::runtime_error(), "Null Python result")
            })?;
            let wrapper = RubyxObject::new(py_result_guard.ptr(), api).ok_or_else(|| {
                magnus::Error::new(
                    ruby_helpers::runtime_error(),
                    "Failed to wrap Python result",
                )
            })?;
            Ok(wrapper.into_value_with(&ruby))
        })();
        api.release_gil(gil);
        result
    }

    pub fn py_type(&self) -> Result<String, magnus::Error> {
        let gil = self.api.ensure_gil();
        let result = self.api.type_name(self.py_obj);
        self.api.release_gil(gil);
        Ok(result.unwrap_or_default())
    }
}

impl Drop for RubyxObject {
    fn drop(&mut self) {
        // Python object no longer exist
        if self.py_obj.is_null() {
            return;
        }
        // Python api does not exist
        if !self.api.is_initialized() {
            return;
        }
        // Lock gil
        let gil = self.api.ensure_gil();
        self.api.decref(self.py_obj);
        self.api.release_gil(gil);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::with_ruby_python;
    use magnus::{IntoValue, TryConvert};
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_ruby_to_python_primitives() {
        with_ruby_python(|ruby, api| {
            let py_nil =
                ruby_to_python(ruby.qnil().as_value(), api).expect("nil conversion should succeed");
            assert!(api.is_none(py_nil));
            api.decref(py_nil);

            let py_true = ruby_to_python(true.into_value_with(ruby), api)
                .expect("true conversion should succeed");
            assert!(api.is_true(py_true));
            api.decref(py_true);

            let py_int = ruby_to_python(42_i64.into_value_with(ruby), api)
                .expect("int conversion should succeed");
            assert_eq!(api.long_to_i64(py_int), 42);
            api.decref(py_int);

            let py_float = ruby_to_python(3.5_f64.into_value_with(ruby), api)
                .expect("float conversion should succeed");
            assert!(api.is_float(py_float));
            assert!((api.float_to_f64(py_float) - 3.5).abs() < 1e-9);
            api.decref(py_float);

            let py_str = ruby_to_python("hello".into_value_with(ruby), api)
                .expect("string conversion should succeed");
            assert_eq!(api.string_to_string(py_str), Some("hello".to_string()));
            api.decref(py_str);
        });
    }

    #[test]
    #[serial]
    fn test_ruby_to_python_symbol() {
        with_ruby_python(|ruby, api| {
            let sym = ruby.sym_new("hello");
            let py_str =
                ruby_to_python(sym.as_value(), api).expect("symbol conversion should succeed");
            assert!(api.is_string(py_str));
            assert_eq!(api.string_to_string(py_str), Some("hello".to_string()));
            api.decref(py_str);
        });
    }

    #[test]
    #[serial]
    fn test_ruby_to_python_false() {
        with_ruby_python(|ruby, api| {
            let py_false = ruby_to_python(false.into_value_with(ruby), api)
                .expect("false conversion should succeed");
            assert!(api.is_false(py_false));
            api.decref(py_false);
        });
    }

    #[test]
    #[serial]
    fn test_ruby_to_python_array() {
        with_ruby_python(|ruby, api| {
            let arr = magnus::RArray::new();
            arr.push(1_i64.into_value_with(ruby)).unwrap();
            arr.push(2_i64.into_value_with(ruby)).unwrap();
            arr.push(3_i64.into_value_with(ruby)).unwrap();
            let py_list = ruby_to_python(arr.into_value_with(ruby), api)
                .expect("array conversion should succeed");
            assert!(api.list_check(py_list));
            assert_eq!(api.list_size(py_list), 3);
            assert_eq!(api.long_to_i64(api.list_get_item(py_list, 0)), 1);
            assert_eq!(api.long_to_i64(api.list_get_item(py_list, 1)), 2);
            assert_eq!(api.long_to_i64(api.list_get_item(py_list, 2)), 3);
            api.decref(py_list);
        });
    }

    #[test]
    #[serial]
    fn test_ruby_to_python_hash() {
        with_ruby_python(|ruby, api| {
            let hash = RHash::new();
            hash.aset(ruby.sym_new("name"), "Alice".into_value_with(ruby))
                .unwrap();
            hash.aset(ruby.sym_new("age"), 30_i64.into_value_with(ruby))
                .unwrap();

            let py_dict = ruby_to_python(hash.into_value_with(ruby), api)
                .expect("hash conversion should succeed");
            assert!(api.dict_check(py_dict));

            let key_name = api.string_from_str("name");
            let val_name = api.dict_get_item(py_dict, key_name);
            assert!(!val_name.is_null());
            assert_eq!(api.string_to_string(val_name), Some("Alice".to_string()));
            api.decref(key_name);

            let key_age = api.string_from_str("age");
            let val_age = api.dict_get_item(py_dict, key_age);
            assert!(!val_age.is_null());
            assert_eq!(api.long_to_i64(val_age), 30);
            api.decref(key_age);

            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_ruby_to_python_nested_array_in_hash() {
        with_ruby_python(|ruby, api| {
            let inner = magnus::RArray::new();
            inner.push(10_i64.into_value_with(ruby)).unwrap();
            inner.push(20_i64.into_value_with(ruby)).unwrap();
            let hash = RHash::new();
            hash.aset(ruby.sym_new("items"), inner.into_value_with(ruby))
                .unwrap();

            let py_dict = ruby_to_python(hash.into_value_with(ruby), api)
                .expect("nested conversion should succeed");
            assert!(api.dict_check(py_dict));

            let key = api.string_from_str("items");
            let py_list = api.dict_get_item(py_dict, key);
            assert!(!py_list.is_null());
            assert!(api.list_check(py_list));
            assert_eq!(api.list_size(py_list), 2);
            assert_eq!(api.long_to_i64(api.list_get_item(py_list, 0)), 10);
            assert_eq!(api.long_to_i64(api.list_get_item(py_list, 1)), 20);

            api.decref(key);
            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_ruby_to_python_empty_array() {
        with_ruby_python(|ruby, api| {
            let arr = magnus::RArray::new();
            let py_list =
                ruby_to_python(arr.into_value_with(ruby), api).expect("empty array should convert");
            assert!(api.list_check(py_list));
            assert_eq!(api.list_size(py_list), 0);
            api.decref(py_list);
        });
    }

    #[test]
    #[serial]
    fn test_ruby_to_python_empty_hash() {
        with_ruby_python(|ruby, api| {
            let hash = RHash::new();
            let py_dict =
                ruby_to_python(hash.into_value_with(ruby), api).expect("empty hash should convert");
            assert!(api.dict_check(py_dict));
            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_ruby_to_python_rubyx_object_passthrough() {
        with_ruby_python(|ruby, api| {
            // Create a Python object via eval
            let globals = crate::eval::make_globals(api);
            let py_obj = api
                .run_string("42", 258, globals.ptr(), globals.ptr())
                .expect("eval should succeed");

            let wrapper = RubyxObject::new(py_obj, api).expect("wrapper should be created");
            let ruby_val = wrapper.into_value_with(ruby);

            let py_result =
                ruby_to_python(ruby_val, api).expect("RubyxObject passthrough should succeed");
            assert_eq!(api.long_to_i64(py_result), 42);
            api.decref(py_result);
            api.decref(py_obj);
        });
    }

    #[test]
    #[serial]
    fn test_method_missing_calls_python_callable() {
        with_ruby_python(|ruby, api| {
            let json = api.import_module("json").expect("json module must import");
            let wrapper = RubyxObject::new(json, api).expect("wrapper should be created");

            let args = vec![
                "loads".into_value_with(ruby),
                "[1, 2, 3]".into_value_with(ruby),
            ];
            let result = wrapper
                .method_missing(&args)
                .expect("loads call should succeed");
            let py_result = Obj::<RubyxObject>::try_convert(result)
                .expect("result should be wrapped Python object");
            assert!(api.list_check(py_result.as_ptr()));
            assert_eq!(api.list_size(py_result.as_ptr()), 3);

            drop(wrapper);
            api.decref(json);
        });
    }

    #[test]
    #[serial]
    fn test_method_missing_reads_non_callable_attribute() {
        with_ruby_python(|ruby, api| {
            let sys = api.import_module("sys").expect("sys module must import");
            let wrapper = RubyxObject::new(sys, api).expect("wrapper should be created");

            let args = vec!["version".into_value_with(ruby)];
            let result = wrapper
                .method_missing(&args)
                .expect("attribute read should succeed");
            let py_result = Obj::<RubyxObject>::try_convert(result)
                .expect("result should be wrapped Python object");
            assert!(api.is_string(py_result.as_ptr()));
            let version = api
                .string_to_string(py_result.as_ptr())
                .expect("version should decode as string");
            assert!(!version.is_empty());
            println!("Python version: {}", version);

            drop(wrapper);
            api.decref(sys);
        });
    }

    #[test]
    #[serial]
    fn test_method_missing_returns_error_for_unknown_member() {
        with_ruby_python(|ruby, api| {
            let sys = api.import_module("sys").expect("sys module must import");
            let wrapper = RubyxObject::new(sys, api).expect("wrapper should be created");

            let args = vec!["this_member_should_not_exist_abc123".into_value_with(ruby)];
            let result = wrapper.method_missing(&args);
            assert!(result.is_err());

            drop(wrapper);
            api.decref(sys);
        });
    }

    // ========== to_s tests ==========

    #[test]
    #[serial]
    fn test_to_s_returns_python_str_for_int() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(99);
        let wrapper = RubyxObject::new(py_int, api).unwrap();
        assert_eq!(wrapper.to_s().unwrap(), "99");
        drop(wrapper);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_to_s_returns_python_str_for_string() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("world");
        let wrapper = RubyxObject::new(py_str, api).unwrap();
        assert_eq!(wrapper.to_s().unwrap(), "world");
        drop(wrapper);
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_to_s_returns_python_str_for_none() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        api.incref(api.py_none);
        let wrapper = RubyxObject::new(api.py_none, api).unwrap();
        assert_eq!(wrapper.to_s().unwrap(), "None");
        drop(wrapper);
    }

    // ========== inspect tests ==========

    #[test]
    #[serial]
    fn test_inspect_returns_repr_for_int() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(7);
        let wrapper = RubyxObject::new(py_int, api).unwrap();
        assert_eq!(wrapper.inspect().unwrap(), "7");
        drop(wrapper);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_inspect_returns_repr_for_string_with_quotes() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("test");
        let wrapper = RubyxObject::new(py_str, api).unwrap();
        // Python repr of string includes quotes
        assert_eq!(wrapper.inspect().unwrap(), "'test'");
        drop(wrapper);
        api.decref(py_str);
    }

    // ========== to_ruby tests ==========

    #[test]
    #[serial]
    fn test_to_ruby_converts_int() {
        with_ruby_python(|_ruby, api| {
            let py_int = api.long_from_i64(123);
            let wrapper = RubyxObject::new(py_int, api).unwrap();
            let ruby_val = wrapper.to_ruby().expect("to_ruby should succeed");
            assert_eq!(i64::try_convert(ruby_val).unwrap(), 123);
            drop(wrapper);
            api.decref(py_int);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_converts_string() {
        with_ruby_python(|_ruby, api| {
            let py_str = api.string_from_str("rubyx");
            let wrapper = RubyxObject::new(py_str, api).unwrap();
            let ruby_val = wrapper.to_ruby().expect("to_ruby should succeed");
            assert_eq!(String::try_convert(ruby_val).unwrap(), "rubyx");
            drop(wrapper);
            api.decref(py_str);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_converts_float() {
        with_ruby_python(|_ruby, api| {
            let py_float = api.float_from_f64(2.718);
            let wrapper = RubyxObject::new(py_float, api).unwrap();
            let ruby_val = wrapper.to_ruby().expect("to_ruby should succeed");
            let f = f64::try_convert(ruby_val).unwrap();
            assert!((f - 2.718).abs() < 0.001);
            drop(wrapper);
            api.decref(py_float);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_converts_bool() {
        with_ruby_python(|_ruby, api| {
            let py_true = api.bool_from_i64(1);
            let wrapper = RubyxObject::new(py_true, api).unwrap();
            let ruby_val = wrapper.to_ruby().expect("to_ruby should succeed");
            assert!(bool::try_convert(ruby_val).unwrap());
            drop(wrapper);
            api.decref(py_true);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_converts_none_to_nil() {
        with_ruby_python(|_ruby, api| {
            api.incref(api.py_none);
            let wrapper = RubyxObject::new(api.py_none, api).unwrap();
            let ruby_val = wrapper.to_ruby().expect("to_ruby should succeed");
            assert!(magnus::value::ReprValue::is_nil(ruby_val));
            drop(wrapper);
        });
    }

    #[test]
    #[serial]
    fn test_to_ruby_wraps_module_as_rubyx_object() {
        with_ruby_python(|_ruby, api| {
            let module = api.import_module("os").expect("os should import");
            let wrapper = RubyxObject::new(module, api).unwrap();
            assert!(
                wrapper.to_ruby().is_ok(),
                "module should convert to RubyxObject via PyObjectRef"
            );
            drop(wrapper);
            api.decref(module);
        });
    }

    // ========== method_missing with args ==========

    #[test]
    #[serial]
    fn test_method_missing_with_no_args_returns_error() {
        with_ruby_python(|_ruby, api| {
            let sys = api.import_module("sys").expect("sys should import");
            let wrapper = RubyxObject::new(sys, api).unwrap();

            let result = wrapper.method_missing(&[]);
            assert!(result.is_err(), "empty args should error");

            drop(wrapper);
            api.decref(sys);
        });
    }

    #[test]
    #[serial]
    fn test_method_missing_chained_calls() {
        with_ruby_python(|ruby, api| {
            let json = api.import_module("json").expect("json should import");
            let wrapper = RubyxObject::new(json, api).unwrap();

            // json.dumps(json.loads("[1,2]"))
            let loads_args = vec![
                "loads".into_value_with(ruby),
                "[1, 2, 3]".into_value_with(ruby),
            ];
            let list_result = wrapper
                .method_missing(&loads_args)
                .expect("loads should succeed");

            let list_wrapper =
                Obj::<RubyxObject>::try_convert(list_result).expect("should be RubyxObject");
            assert!(api.list_check(list_wrapper.as_ptr()));

            drop(wrapper);
            api.decref(json);
        });
    }

    // ========== respond_to_missing? tests ==========

    #[test]
    #[serial]
    fn test_respond_to_missing_existing_attr() {
        with_ruby_python(|ruby, api| {
            let sys = api.import_module("sys").expect("sys should import");
            let wrapper = RubyxObject::new(sys, api).unwrap();

            // sys.version exists
            let args = vec!["version".into_value_with(ruby)];
            let result = wrapper.respond_to_missing(&args).expect("should not error");
            assert!(result, "sys.version should exist");

            drop(wrapper);
            api.decref(sys);
        });
    }

    #[test]
    #[serial]
    fn test_respond_to_missing_nonexistent_attr() {
        with_ruby_python(|ruby, api| {
            let sys = api.import_module("sys").expect("sys should import");
            let wrapper = RubyxObject::new(sys, api).unwrap();

            let args = vec!["nonexistent_xyz_123".into_value_with(ruby)];
            let result = wrapper.respond_to_missing(&args).expect("should not error");
            assert!(!result, "nonexistent attr should return false");

            drop(wrapper);
            api.decref(sys);
        });
    }

    #[test]
    #[serial]
    fn test_respond_to_missing_callable_method() {
        with_ruby_python(|ruby, api| {
            let json = api.import_module("json").expect("json should import");
            let wrapper = RubyxObject::new(json, api).unwrap();

            let args = vec!["loads".into_value_with(ruby)];
            let result = wrapper.respond_to_missing(&args).expect("should not error");
            assert!(result, "json.loads should exist");

            drop(wrapper);
            api.decref(json);
        });
    }

    #[test]
    #[serial]
    fn test_respond_to_missing_with_string_arg() {
        with_ruby_python(|ruby, api| {
            let sys = api.import_module("sys").expect("sys should import");
            let wrapper = RubyxObject::new(sys, api).unwrap();

            // Pass string instead of symbol
            let args = vec!["version".into_value_with(ruby)];
            let result = wrapper.respond_to_missing(&args).expect("should not error");
            assert!(result, "should accept string arg too");

            drop(wrapper);
            api.decref(sys);
        });
    }

    #[test]
    #[serial]
    fn test_respond_to_missing_empty_args_errors() {
        with_ruby_python(|_ruby, api| {
            let sys = api.import_module("sys").expect("sys should import");
            let wrapper = RubyxObject::new(sys, api).unwrap();

            let result = wrapper.respond_to_missing(&[]);
            assert!(result.is_err(), "empty args should error");

            drop(wrapper);
            api.decref(sys);
        });
    }

    // ========== implicit conversion guards ==========

    #[test]
    #[serial]
    fn test_method_missing_guards_to_ary() {
        with_ruby_python(|ruby, api| {
            let py_int = api.long_from_i64(42);
            let wrapper = RubyxObject::new(py_int, api).unwrap();

            let args = vec!["to_ary".into_value_with(ruby)];
            let result = wrapper.method_missing(&args);
            assert!(result.is_err(), "to_ary should be guarded");

            drop(wrapper);
            api.decref(py_int);
        });
    }

    #[test]
    #[serial]
    fn test_method_missing_guards_to_str() {
        with_ruby_python(|ruby, api| {
            let py_int = api.long_from_i64(42);
            let wrapper = RubyxObject::new(py_int, api).unwrap();

            let args = vec!["to_str".into_value_with(ruby)];
            let result = wrapper.method_missing(&args);
            assert!(result.is_err(), "to_str should be guarded");

            drop(wrapper);
            api.decref(py_int);
        });
    }

    #[test]
    #[serial]
    fn test_method_missing_guards_to_hash() {
        with_ruby_python(|ruby, api| {
            let py_int = api.long_from_i64(42);
            let wrapper = RubyxObject::new(py_int, api).unwrap();

            let args = vec!["to_hash".into_value_with(ruby)];
            let result = wrapper.method_missing(&args);
            assert!(result.is_err(), "to_hash should be guarded");

            drop(wrapper);
            api.decref(py_int);
        });
    }

    #[test]
    #[serial]
    fn test_method_missing_guards_to_int() {
        with_ruby_python(|ruby, api| {
            let py_int = api.long_from_i64(42);
            let wrapper = RubyxObject::new(py_int, api).unwrap();

            let args = vec!["to_int".into_value_with(ruby)];
            let result = wrapper.method_missing(&args);
            assert!(result.is_err(), "to_int should be guarded");

            drop(wrapper);
            api.decref(py_int);
        });
    }

    #[test]
    #[serial]
    fn test_method_missing_allows_regular_methods() {
        with_ruby_python(|ruby, api| {
            let sys = api.import_module("sys").expect("sys should import");
            let wrapper = RubyxObject::new(sys, api).unwrap();

            // "version" is not guarded — should delegate to Python
            let args = vec!["version".into_value_with(ruby)];
            let result = wrapper.method_missing(&args);
            assert!(result.is_ok(), "regular attributes should not be guarded");

            drop(wrapper);
            api.decref(sys);
        });
    }

    // ========== getitem / setitem / delitem tests ==========

    #[test]
    #[serial]
    fn test_getitem_dict_string_key() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_dict = api
                .run_string(
                    "{'name': 'Alice', 'age': 30}",
                    258,
                    globals.ptr(),
                    globals.ptr(),
                )
                .expect("should create dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            let key: magnus::Value = "name".into_value_with(ruby);
            let result = wrapper.getitem(key).expect("getitem should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(
                api.string_to_string(obj.as_ptr()),
                Some("Alice".to_string())
            );

            drop(wrapper);
            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_getitem_dict_integer_key() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_dict = api
                .run_string("{1: 'one', 2: 'two'}", 258, globals.ptr(), globals.ptr())
                .expect("should create dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            let key: magnus::Value = 1_i64.into_value_with(ruby);
            let result = wrapper.getitem(key).expect("getitem should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.string_to_string(obj.as_ptr()), Some("one".to_string()));

            drop(wrapper);
            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_getitem_list_by_index() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_list = api
                .run_string("[10, 20, 30]", 258, globals.ptr(), globals.ptr())
                .expect("should create list");
            let wrapper = RubyxObject::new(py_list, api).unwrap();

            let key: magnus::Value = 1_i64.into_value_with(ruby);
            let result = wrapper.getitem(key).expect("getitem should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 20);

            drop(wrapper);
            api.decref(py_list);
        });
    }

    #[test]
    #[serial]
    fn test_getitem_list_negative_index() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_list = api
                .run_string("[10, 20, 30]", 258, globals.ptr(), globals.ptr())
                .expect("should create list");
            let wrapper = RubyxObject::new(py_list, api).unwrap();

            // Python supports negative indexing
            let key: magnus::Value = (-1_i64).into_value_with(ruby);
            let result = wrapper.getitem(key).expect("getitem should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 30);

            drop(wrapper);
            api.decref(py_list);
        });
    }

    #[test]
    #[serial]
    fn test_getitem_missing_key_raises_error() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_dict = api
                .run_string("{}", 258, globals.ptr(), globals.ptr())
                .expect("should create empty dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            let key: magnus::Value = "nope".into_value_with(ruby);
            let result = wrapper.getitem(key);
            assert!(result.is_err(), "missing key should raise error");

            drop(wrapper);
            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_getitem_index_out_of_range() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_list = api
                .run_string("[1, 2]", 258, globals.ptr(), globals.ptr())
                .expect("should create list");
            let wrapper = RubyxObject::new(py_list, api).unwrap();

            let key: magnus::Value = 99_i64.into_value_with(ruby);
            let result = wrapper.getitem(key);
            assert!(result.is_err(), "out of range index should raise error");

            drop(wrapper);
            api.decref(py_list);
        });
    }

    #[test]
    #[serial]
    fn test_setitem_dict() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_dict = api
                .run_string("{}", 258, globals.ptr(), globals.ptr())
                .expect("should create empty dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            let key: magnus::Value = "role".into_value_with(ruby);
            let val: magnus::Value = "admin".into_value_with(ruby);
            wrapper.setitem(key, val).expect("setitem should succeed");

            // Verify the value was set
            let check_key: magnus::Value = "role".into_value_with(ruby);
            let result = wrapper.getitem(check_key).expect("should find new key");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(
                api.string_to_string(obj.as_ptr()),
                Some("admin".to_string())
            );

            drop(wrapper);
            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_setitem_list() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_list = api
                .run_string("[1, 2, 3]", 258, globals.ptr(), globals.ptr())
                .expect("should create list");
            let wrapper = RubyxObject::new(py_list, api).unwrap();

            let key: magnus::Value = 1_i64.into_value_with(ruby);
            let val: magnus::Value = 99_i64.into_value_with(ruby);
            wrapper.setitem(key, val).expect("setitem should succeed");

            // Verify
            let check_key: magnus::Value = 1_i64.into_value_with(ruby);
            let result = wrapper.getitem(check_key).expect("should read index 1");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 99);

            drop(wrapper);
            api.decref(py_list);
        });
    }

    #[test]
    #[serial]
    fn test_setitem_overwrite_existing() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_dict = api
                .run_string("{'x': 1}", 258, globals.ptr(), globals.ptr())
                .expect("should create dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            let key: magnus::Value = "x".into_value_with(ruby);
            let val: magnus::Value = 42_i64.into_value_with(ruby);
            wrapper.setitem(key, val).expect("setitem should succeed");

            let check_key: magnus::Value = "x".into_value_with(ruby);
            let result = wrapper.getitem(check_key).expect("should read key");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 42);

            drop(wrapper);
            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_delitem_dict() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_dict = api
                .run_string("{'a': 1, 'b': 2}", 258, globals.ptr(), globals.ptr())
                .expect("should create dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            let key: magnus::Value = "a".into_value_with(ruby);
            wrapper.delitem(key).expect("delitem should succeed");

            // Verify 'a' is gone
            let check_key: magnus::Value = "a".into_value_with(ruby);
            let result = wrapper.getitem(check_key);
            assert!(result.is_err(), "'a' should be deleted");

            // Verify 'b' still exists
            let check_key_b: magnus::Value = "b".into_value_with(ruby);
            let result_b = wrapper
                .getitem(check_key_b)
                .expect("'b' should still exist");
            let obj = Obj::<RubyxObject>::try_convert(result_b).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 2);

            drop(wrapper);
            api.decref(py_dict);
        });
    }

    #[test]
    #[serial]
    fn test_delitem_missing_key_raises_error() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_dict = api
                .run_string("{}", 258, globals.ptr(), globals.ptr())
                .expect("should create empty dict");
            let wrapper = RubyxObject::new(py_dict, api).unwrap();

            let key: magnus::Value = "nope".into_value_with(ruby);
            let result = wrapper.delitem(key);
            assert!(result.is_err(), "deleting missing key should error");

            drop(wrapper);
            api.decref(py_dict);
        });
    }

    // ========== call tests ==========

    #[test]
    #[serial]
    fn test_call_lambda_no_args() {
        with_ruby_python(|_ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_func = api
                .run_string("lambda: 42", 258, globals.ptr(), globals.ptr())
                .expect("lambda eval should succeed");
            let wrapper = RubyxObject::new(py_func, api).unwrap();

            let result = wrapper.call(&[]).expect("call should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 42);

            drop(wrapper);
            api.decref(py_func);
        });
    }

    #[test]
    #[serial]
    fn test_call_lambda_with_args() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_func = api
                .run_string("lambda x, y: x + y", 258, globals.ptr(), globals.ptr())
                .expect("lambda eval should succeed");
            let wrapper = RubyxObject::new(py_func, api).unwrap();

            let args = vec![3_i64.into_value_with(ruby), 4_i64.into_value_with(ruby)];
            let result = wrapper.call(&args).expect("call should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 7);

            drop(wrapper);
            api.decref(py_func);
        });
    }

    #[test]
    #[serial]
    fn test_call_builtin_function() {
        with_ruby_python(|ruby, api| {
            let builtins = api
                .import_module("builtins")
                .expect("builtins should import");
            let len_func = api.object_get_attr_string(builtins, "len");
            let wrapper = RubyxObject::new(len_func, api).unwrap();

            let globals = crate::eval::make_globals(api);
            let py_list = api
                .run_string("[1, 2, 3]", 258, globals.ptr(), globals.ptr())
                .expect("list eval should succeed");
            let list_wrapper = RubyxObject::new(py_list, api).unwrap();

            let args = vec![list_wrapper.into_value_with(ruby)];
            let result = wrapper.call(&args).expect("call should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(api.long_to_i64(obj.as_ptr()), 3);

            drop(wrapper);
            api.decref(len_func);
            api.decref(builtins);
            api.decref(py_list);
        });
    }

    #[test]
    #[serial]
    fn test_call_with_kwargs() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let _ = api.run_string(
                "def greet(name, greeting='Hello'): return f'{greeting}, {name}!'",
                257,
                globals.ptr(),
                globals.ptr(),
            );
            let key = api.string_from_str("greet");
            let func = api.dict_get_item(globals.ptr(), key);
            api.decref(key);
            let wrapper = RubyxObject::new(func, api).unwrap();

            let kwargs = ruby.hash_new();
            kwargs
                .aset(ruby.sym_new("greeting"), "Hi".into_value_with(ruby))
                .unwrap();
            let args = vec!["Alice".into_value_with(ruby), kwargs.into_value_with(ruby)];
            let result = wrapper
                .call(&args)
                .expect("call with kwargs should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert_eq!(
                api.string_to_string(obj.as_ptr()),
                Some("Hi, Alice!".to_string())
            );

            drop(wrapper);
        });
    }

    #[test]
    #[serial]
    fn test_call_class_as_constructor() {
        with_ruby_python(|ruby, api| {
            let globals = crate::eval::make_globals(api);
            let _ = api.run_string(
                "class Pt:\n    def __init__(self, x):\n        self.x = x",
                257,
                globals.ptr(),
                globals.ptr(),
            );
            let key = api.string_from_str("Pt");
            let cls = api.dict_get_item(globals.ptr(), key);
            api.decref(key);
            let wrapper = RubyxObject::new(cls, api).unwrap();

            let args = vec![10_i64.into_value_with(ruby)];
            let result = wrapper.call(&args).expect("class call should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");

            let x_attr = api.object_get_attr_string(obj.as_ptr(), "x");
            assert!(!x_attr.is_null());
            assert_eq!(api.long_to_i64(x_attr), 10);
            api.decref(x_attr);

            drop(wrapper);
        });
    }

    #[test]
    #[serial]
    fn test_call_non_callable_raises_error() {
        with_ruby_python(|_ruby, api| {
            let py_int = api.long_from_i64(42);
            let wrapper = RubyxObject::new(py_int, api).unwrap();

            let result = wrapper.call(&[]);
            assert!(result.is_err(), "calling non-callable should error");
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("not callable"),
                "error should mention not callable, got: {err_msg}"
            );

            drop(wrapper);
            api.decref(py_int);
        });
    }

    #[test]
    #[serial]
    fn test_call_propagates_python_error() {
        with_ruby_python(|_ruby, api| {
            let globals = crate::eval::make_globals(api);
            let _ = api.run_string(
                "def explode(): raise ValueError('boom')",
                257,
                globals.ptr(),
                globals.ptr(),
            );
            let key = api.string_from_str("explode");
            let func = api.dict_get_item(globals.ptr(), key);
            api.decref(key);
            let wrapper = RubyxObject::new(func, api).unwrap();

            let result = wrapper.call(&[]);
            assert!(result.is_err(), "call that raises should return error");
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("boom"),
                "error should contain Python message, got: {err_msg}"
            );

            drop(wrapper);
        });
    }

    #[test]
    #[serial]
    fn test_call_returns_rubyx_object() {
        with_ruby_python(|_ruby, api| {
            let globals = crate::eval::make_globals(api);
            let py_func = api
                .run_string("lambda: [1, 2, 3]", 258, globals.ptr(), globals.ptr())
                .expect("lambda eval should succeed");
            let wrapper = RubyxObject::new(py_func, api).unwrap();

            let result = wrapper.call(&[]).expect("call should succeed");
            let obj = Obj::<RubyxObject>::try_convert(result).expect("should be RubyxObject");
            assert!(api.list_check(obj.as_ptr()));
            assert_eq!(api.list_size(obj.as_ptr()), 3);

            drop(wrapper);
            api.decref(py_func);
        });
    }

    // ========== is_truthy / is_falsy tests ==========

    #[test]
    #[serial]
    fn test_truthy_nonzero_int() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.long_from_i64(42), api).unwrap();
        assert!(w.is_truthy());
        assert!(!w.is_falsy());
    }

    #[test]
    #[serial]
    fn test_truthy_zero_int() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.long_from_i64(0), api).unwrap();
        assert!(!w.is_truthy());
        assert!(w.is_falsy());
    }

    #[test]
    #[serial]
    fn test_truthy_none() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.incref(api.py_none);
        let w = RubyxObject::new(api.py_none, api).unwrap();
        assert!(!w.is_truthy());
        assert!(w.is_falsy());
    }

    #[test]
    #[serial]
    fn test_truthy_bool() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let t = RubyxObject::new(api.bool_from_i64(1), api).unwrap();
        assert!(t.is_truthy());
        let f = RubyxObject::new(api.bool_from_i64(0), api).unwrap();
        assert!(f.is_falsy());
    }

    #[test]
    #[serial]
    fn test_truthy_empty_string() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.string_from_str(""), api).unwrap();
        assert!(w.is_falsy());
    }

    #[test]
    #[serial]
    fn test_truthy_nonempty_string() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.string_from_str("hello"), api).unwrap();
        assert!(w.is_truthy());
    }

    #[test]
    #[serial]
    fn test_truthy_empty_list() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let list = unsafe { (api.py_list_new)(0) };
        let w = RubyxObject::new(list, api).unwrap();
        assert!(w.is_falsy());
    }

    #[test]
    #[serial]
    fn test_truthy_nonempty_list() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let list = unsafe { (api.py_list_new)(1) };
        unsafe { (api.py_list_set_item)(list, 0, api.long_from_i64(1)) };
        let w = RubyxObject::new(list, api).unwrap();
        assert!(w.is_truthy());
    }

    // ========== is_callable tests ==========

    #[test]
    #[serial]
    fn test_callable_function() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let json = api.import_module("json").expect("json should import");
        let loads = api.object_get_attr_string(json, "loads");
        let w = RubyxObject::new(loads, api).unwrap();
        assert!(w.is_callable());
        drop(w);
        api.decref(loads);
        api.decref(json);
    }

    #[test]
    #[serial]
    fn test_not_callable_int() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.long_from_i64(42), api).unwrap();
        assert!(!w.is_callable());
    }

    #[test]
    #[serial]
    fn test_not_callable_string() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.string_from_str("hi"), api).unwrap();
        assert!(!w.is_callable());
    }

    #[test]
    #[serial]
    fn test_not_callable_module() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let os = api.import_module("os").expect("os should import");
        let w = RubyxObject::new(os, api).unwrap();
        assert!(!w.is_callable());
        drop(w);
        api.decref(os);
    }

    // ========== py_type tests ==========

    #[test]
    #[serial]
    fn test_py_type_int() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.long_from_i64(1), api).unwrap();
        assert_eq!(w.py_type().unwrap(), "int");
    }

    #[test]
    #[serial]
    fn test_py_type_str() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.string_from_str("x"), api).unwrap();
        assert_eq!(w.py_type().unwrap(), "str");
    }

    #[test]
    #[serial]
    fn test_py_type_float() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.float_from_f64(1.0), api).unwrap();
        assert_eq!(w.py_type().unwrap(), "float");
    }

    #[test]
    #[serial]
    fn test_py_type_bool() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let w = RubyxObject::new(api.bool_from_i64(1), api).unwrap();
        assert_eq!(w.py_type().unwrap(), "bool");
    }

    #[test]
    #[serial]
    fn test_py_type_list() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let list = unsafe { (api.py_list_new)(0) };
        let w = RubyxObject::new(list, api).unwrap();
        assert_eq!(w.py_type().unwrap(), "list");
    }

    #[test]
    #[serial]
    fn test_py_type_dict() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let dict = api.dict_new();
        let w = RubyxObject::new(dict, api).unwrap();
        assert_eq!(w.py_type().unwrap(), "dict");
    }

    #[test]
    #[serial]
    fn test_py_type_none() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.incref(api.py_none);
        let w = RubyxObject::new(api.py_none, api).unwrap();
        assert_eq!(w.py_type().unwrap(), "NoneType");
    }

    #[test]
    #[serial]
    fn test_py_type_module() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let os = api.import_module("os").expect("os should import");
        let w = RubyxObject::new(os, api).unwrap();
        assert_eq!(w.py_type().unwrap(), "module");
        drop(w);
        api.decref(os);
    }

    // ========== python_to_sendable: PyObjectRef fallback ==========

    #[test]
    #[serial]
    fn test_python_to_sendable_module_returns_py_object_ref() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let os = api.import_module("os").expect("os should import");
        let sendable = python_to_sendable(os, api).unwrap();
        match &sendable {
            SendableValue::PyObjectRef(addr) => {
                assert_eq!(*addr, os as usize);
            }
            other => panic!("expected PyObjectRef, got {other:?}"),
        }
        // Clean up: decref once for the sendable's incref, once for import_module
        api.decref(os);
        api.decref(os);
    }

    // ========== python_to_sendable: set / frozenset ==========

    #[test]
    #[serial]
    fn test_python_to_sendable_set_returns_set() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = api.dict_new();
        let builtins = api
            .import_module("builtins")
            .expect("builtins should import");
        let key = api.string_from_str("__builtins__");
        api.dict_set_item(globals, key, builtins);
        api.decref(key);
        let result = api.run_string("{1, 2, 3}", 258, globals, globals);
        let py_set = result.expect("set eval should succeed");
        assert!(!py_set.is_null());

        let sendable = python_to_sendable(py_set, api).expect("set should convert to Set");
        match &sendable {
            SendableValue::Set(items) => {
                assert_eq!(items.len(), 3);
                let mut vals: Vec<i64> = items
                    .iter()
                    .map(|item| match item {
                        SendableValue::Integer(n) => *n,
                        other => panic!("expected Integer, got {other:?}"),
                    })
                    .collect();
                vals.sort();
                assert_eq!(vals, vec![1, 2, 3]);
            }
            other => panic!("expected Set, got {other:?}"),
        }
        api.decref(py_set);
        api.decref(builtins);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_frozenset_returns_set() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = api.dict_new();
        let builtins = api
            .import_module("builtins")
            .expect("builtins should import");
        let key = api.string_from_str("__builtins__");
        api.dict_set_item(globals, key, builtins);
        api.decref(key);
        let result = api.run_string("frozenset({10, 20})", 258, globals, globals);
        let py_fset = result.expect("frozenset eval should succeed");
        assert!(!py_fset.is_null());

        let sendable = python_to_sendable(py_fset, api).expect("frozenset should convert to Set");
        match &sendable {
            SendableValue::Set(items) => {
                assert_eq!(items.len(), 2);
                let mut vals: Vec<i64> = items
                    .iter()
                    .map(|item| match item {
                        SendableValue::Integer(n) => *n,
                        other => panic!("expected Integer, got {other:?}"),
                    })
                    .collect();
                vals.sort();
                assert_eq!(vals, vec![10, 20]);
            }
            other => panic!("expected Set, got {other:?}"),
        }
        api.decref(py_fset);
        api.decref(builtins);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_empty_set() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = api.dict_new();
        let builtins = api
            .import_module("builtins")
            .expect("builtins should import");
        let key = api.string_from_str("__builtins__");
        api.dict_set_item(globals, key, builtins);
        api.decref(key);
        let result = api.run_string("set()", 258, globals, globals);
        let py_set = result.expect("empty set eval should succeed");
        assert!(!py_set.is_null());

        let sendable = python_to_sendable(py_set, api).expect("empty set should convert");
        match &sendable {
            SendableValue::Set(items) => assert!(items.is_empty()),
            other => panic!("expected empty Set, got {other:?}"),
        }
        api.decref(py_set);
        api.decref(builtins);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_set_with_mixed_types() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = api.dict_new();
        let builtins = api
            .import_module("builtins")
            .expect("builtins should import");
        let key = api.string_from_str("__builtins__");
        api.dict_set_item(globals, key, builtins);
        api.decref(key);
        let result = api.run_string("{42, 'hello', 3.14, True}", 258, globals, globals);
        let py_set = result.expect("mixed set eval should succeed");
        assert!(!py_set.is_null());

        let sendable = python_to_sendable(py_set, api).expect("mixed set should convert");
        match &sendable {
            SendableValue::Set(items) => {
                assert_eq!(items.len(), 4);
                let has_int = items
                    .iter()
                    .any(|i| matches!(i, SendableValue::Integer(42)));
                let has_str = items
                    .iter()
                    .any(|i| matches!(i, SendableValue::Str(s) if s == "hello"));
                let has_float = items.iter().any(|i| matches!(i, SendableValue::Float(_)));
                let has_bool = items.iter().any(|i| matches!(i, SendableValue::Bool(true)));
                assert!(has_int, "set should contain integer 42");
                assert!(has_str, "set should contain string 'hello'");
                assert!(has_float, "set should contain float 3.14");
                assert!(has_bool, "set should contain bool True");
            }
            other => panic!("expected Set, got {other:?}"),
        }
        api.decref(py_set);
        api.decref(builtins);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_set_with_strings() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = api.dict_new();
        let builtins = api
            .import_module("builtins")
            .expect("builtins should import");
        let key = api.string_from_str("__builtins__");
        api.dict_set_item(globals, key, builtins);
        api.decref(key);
        let result = api.run_string("{'apple', 'banana', 'cherry'}", 258, globals, globals);
        let py_set = result.expect("string set eval should succeed");
        assert!(!py_set.is_null());

        let sendable = python_to_sendable(py_set, api).expect("string set should convert");
        match &sendable {
            SendableValue::Set(items) => {
                assert_eq!(items.len(), 3);
                let mut vals: Vec<&str> = items
                    .iter()
                    .map(|item| match item {
                        SendableValue::Str(s) => s.as_str(),
                        other => panic!("expected Str, got {other:?}"),
                    })
                    .collect();
                vals.sort();
                assert_eq!(vals, vec!["apple", "banana", "cherry"]);
            }
            other => panic!("expected Set, got {other:?}"),
        }
        api.decref(py_set);
        api.decref(builtins);
        api.decref(globals);
    }

    // ========== python_to_sendable: callable ==========

    #[test]
    #[serial]
    fn test_python_to_sendable_user_defined_function() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = crate::eval::make_globals(api);
        let py_func = api.run_string(
            "def greet(name): return f'Hello {name}'\ngreet",
            257, // Py_file_input for statements
            globals.ptr(),
            globals.ptr(),
        );
        // file_input returns None; retrieve the function from globals
        drop(py_func);
        let key = api.string_from_str("greet");
        let func = api.dict_get_item(globals.ptr(), key);
        api.decref(key);
        assert!(!func.is_null(), "greet function should exist in globals");
        assert!(api.callable_check(func) != 0, "greet should be callable");

        let sendable = python_to_sendable(func, api)
            .expect("user-defined function should convert via PyObjectRef");
        match &sendable {
            SendableValue::PyObjectRef(addr) => {
                assert_eq!(*addr, func as usize);
            }
            other => panic!("expected PyObjectRef for function, got {other:?}"),
        }
        // Clean up: decref the incref from python_to_sendable
        // dict_get_item returns a borrowed ref, so only the sendable's incref needs cleanup
        api.decref(func);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_lambda() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = crate::eval::make_globals(api);
        let py_lambda = api
            .run_string("lambda x: x * 2", 258, globals.ptr(), globals.ptr())
            .expect("lambda eval should succeed");
        assert!(!py_lambda.is_null());
        assert!(
            api.callable_check(py_lambda) != 0,
            "lambda should be callable"
        );

        let sendable =
            python_to_sendable(py_lambda, api).expect("lambda should convert via PyObjectRef");
        match &sendable {
            SendableValue::PyObjectRef(addr) => {
                assert_eq!(*addr, py_lambda as usize);
            }
            other => panic!("expected PyObjectRef for lambda, got {other:?}"),
        }
        api.decref(py_lambda); // sendable's incref
        api.decref(py_lambda); // run_string's ref
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_builtin_function() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let builtins = api
            .import_module("builtins")
            .expect("builtins should import");
        let len_func = api.object_get_attr_string(builtins, "len");
        assert!(!len_func.is_null(), "len should be accessible");
        assert!(api.callable_check(len_func) != 0, "len should be callable");

        let sendable = python_to_sendable(len_func, api)
            .expect("builtin function should convert via PyObjectRef");
        match &sendable {
            SendableValue::PyObjectRef(addr) => {
                assert_eq!(*addr, len_func as usize);
            }
            other => panic!("expected PyObjectRef for builtin function, got {other:?}"),
        }
        api.decref(len_func); // sendable's incref
        api.decref(len_func); // get_attr_string ref
        api.decref(builtins);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_class() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = crate::eval::make_globals(api);
        // Define a class (classes are callable — calling them constructs instances)
        let _ = api.run_string(
            "class Greeter:\n    def __init__(self, name):\n        self.name = name",
            257,
            globals.ptr(),
            globals.ptr(),
        );
        let key = api.string_from_str("Greeter");
        let cls = api.dict_get_item(globals.ptr(), key);
        api.decref(key);
        assert!(!cls.is_null(), "Greeter class should exist in globals");
        assert!(api.callable_check(cls) != 0, "classes should be callable");

        let sendable = python_to_sendable(cls, api).expect("class should convert via PyObjectRef");
        match &sendable {
            SendableValue::PyObjectRef(addr) => {
                assert_eq!(*addr, cls as usize);
            }
            other => panic!("expected PyObjectRef for class, got {other:?}"),
        }
        api.decref(cls); // sendable's incref
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_instance_with_call() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = crate::eval::make_globals(api);
        // Create an instance with __call__
        let py_obj = api
            .run_string(
                "type('Adder', (), {'__call__': lambda self, x, y: x + y})()",
                258,
                globals.ptr(),
                globals.ptr(),
            )
            .expect("callable instance eval should succeed");
        assert!(!py_obj.is_null());
        assert!(
            api.callable_check(py_obj) != 0,
            "instance with __call__ should be callable"
        );

        let sendable = python_to_sendable(py_obj, api)
            .expect("callable instance should convert via PyObjectRef");
        match &sendable {
            SendableValue::PyObjectRef(addr) => {
                assert_eq!(*addr, py_obj as usize);
            }
            other => panic!("expected PyObjectRef for callable instance, got {other:?}"),
        }
        api.decref(py_obj); // sendable's incref
        api.decref(py_obj); // run_string ref
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_callable_is_callable_check() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = crate::eval::make_globals(api);
        let py_lambda = api
            .run_string("lambda: 42", 258, globals.ptr(), globals.ptr())
            .expect("lambda eval should succeed");

        let sendable = python_to_sendable(py_lambda, api).expect("lambda should convert");
        match &sendable {
            SendableValue::PyObjectRef(addr) => {
                // Verify the wrapped object is still callable
                let ptr = *addr as *mut PyObject;
                assert!(
                    api.callable_check(ptr) != 0,
                    "wrapped callable should still report callable"
                );
            }
            other => panic!("expected PyObjectRef, got {other:?}"),
        }
        api.decref(py_lambda);
        api.decref(py_lambda);
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_callable_method() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        // Get a bound method: "hello".upper
        let globals = crate::eval::make_globals(api);
        let py_method = api
            .run_string("'hello'.upper", 258, globals.ptr(), globals.ptr())
            .expect("bound method eval should succeed");
        assert!(!py_method.is_null());
        assert!(
            api.callable_check(py_method) != 0,
            "bound method should be callable"
        );

        let sendable = python_to_sendable(py_method, api)
            .expect("bound method should convert via PyObjectRef");
        assert!(
            matches!(sendable, SendableValue::PyObjectRef(_)),
            "bound method should be PyObjectRef"
        );
        api.decref(py_method); // sendable's incref
        api.decref(py_method); // run_string ref
    }

    #[test]
    #[serial]
    fn test_python_to_sendable_primitives_not_py_object_ref() {
        use crate::test_helpers::skip_if_no_python;
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // int → Integer, not PyObjectRef
        let py_int = api.long_from_i64(42);
        assert!(matches!(
            python_to_sendable(py_int, api),
            Ok(SendableValue::Integer(42))
        ));

        // str → Str, not PyObjectRef
        let py_str = api.string_from_str("hello");
        assert!(matches!(
            python_to_sendable(py_str, api),
            Ok(SendableValue::Str(_))
        ));

        // None → Nil, not PyObjectRef
        assert!(matches!(
            python_to_sendable(api.py_none, api),
            Ok(SendableValue::Nil)
        ));
    }
}
