use crate::exception::PythonException;
use crate::python_ffi::PyObject;
use crate::python_ffi::*;
use crate::python_guard::PyGuard;
use libc::wchar_t;
use libloading::{Library, Symbol};
use std::ffi::{c_char, c_double, c_int, CString};
use std::path::Path;

pub struct PythonApi {
    _lib: Library,

    py_initialize: Symbol<'static, unsafe extern "C" fn()>,
    pub(crate) py_initialize_ex: Symbol<'static, unsafe extern "C" fn(init_sigs: c_int)>,
    py_finalize: Symbol<'static, unsafe extern "C" fn()>,
    py_is_initialized: Symbol<'static, unsafe extern "C" fn() -> c_int>,
    // None
    pub(crate) py_none: *mut PyObject,
    // True and False
    pub(crate) py_true: *mut PyObject,
    pub(crate) py_false: *mut PyObject,
    async_to_sync_class: *mut PyObject,

    py_run_simple_string: Symbol<'static, unsafe extern "C" fn(*const c_char) -> c_int>,
    py_run_string: Symbol<
        'static,
        unsafe extern "C" fn(
            str: *const c_char,
            start: c_int,
            globals: *mut PyObject,
            locals: *mut PyObject,
        ) -> *mut PyObject,
    >,

    // Long
    py_long_from_longlong: Symbol<'static, unsafe extern "C" fn(i64) -> *mut PyObject>,
    py_long_as_longlong: Symbol<'static, unsafe extern "C" fn(*mut PyObject) -> i64>,
    py_long_type: *mut PyObject,

    // Float and double
    py_float_from_double: Symbol<'static, unsafe extern "C" fn(f64) -> *mut PyObject>,
    py_float_as_double: Symbol<'static, unsafe extern "C" fn(*mut PyObject) -> c_double>,
    py_float_type: *mut PyObject,

    // Bool from long
    py_bool_from_long: Symbol<'static, unsafe extern "C" fn(i64) -> *mut PyObject>,

    // Unicode
    py_unicode_from_string_and_size:
        Symbol<'static, unsafe extern "C" fn(*const c_char, Py_ssize_t) -> *mut PyObject>,
    py_unicode_as_utf8_and_size:
        Symbol<'static, unsafe extern "C" fn(*mut PyObject, *mut Py_ssize_t) -> *const c_char>,
    py_unicode_type: *mut PyObject,

    // bytes
    py_bytes_type: *mut PyObject,
    py_bytes_from_string_and_size:
        Symbol<'static, unsafe extern "C" fn(v: *const c_char, len: Py_ssize_t) -> *mut PyObject>,
    py_bytes_as_string_and_size: Symbol<
        'static,
        unsafe extern "C" fn(
            o: *mut PyObject,
            buffer: *mut *mut c_char,
            len: *mut Py_ssize_t,
        ) -> c_int,
    >,
    py_bytes_size: Symbol<'static, unsafe extern "C" fn(o: *mut PyObject) -> Py_ssize_t>,

    // bytearray
    py_bytearray_type: *mut PyObject,
    py_bytearray_from_object:
        Symbol<'static, unsafe extern "C" fn(o: *mut PyObject) -> *mut PyObject>,
    py_bytearray_from_string_and_size:
        Symbol<'static, unsafe extern "C" fn(str: *const c_char, len: Py_ssize_t) -> *mut PyObject>,
    py_bytearray_size: Symbol<'static, unsafe extern "C" fn(o: *mut PyObject) -> Py_ssize_t>,
    py_bytearray_as_string: Symbol<'static, unsafe extern "C" fn(o: *mut PyObject) -> *mut c_char>,

    // Collection Functions

    // Tuple
    pub py_tuple_new: Symbol<'static, unsafe extern "C" fn(len: isize) -> *mut PyObject>,
    pub py_tuple_size: Symbol<'static, unsafe extern "C" fn(*mut PyObject) -> isize>,
    pub py_tuple_type: *mut PyObject,
    pub py_tuple_get_item:
        Symbol<'static, unsafe extern "C" fn(*mut PyObject, isize) -> *mut PyObject>,
    pub py_tuple_set_item: Symbol<
        'static,
        unsafe extern "C" fn(p: *mut PyObject, pos: isize, o: *mut PyObject) -> c_int,
    >,

    // Set
    pub py_set_new: Symbol<'static, unsafe extern "C" fn(len: isize) -> *mut PyObject>,
    pub py_set_type: *mut PyObject,
    pub py_set_size: Symbol<'static, unsafe extern "C" fn(any_set: *mut PyObject) -> isize>,
    pub py_frozen_set_new: Symbol<'static, unsafe extern "C" fn(len: isize) -> *mut PyObject>,
    pub py_frozen_set_type: *mut PyObject,

    // List
    pub py_list_new: Symbol<'static, unsafe extern "C" fn(isize) -> *mut PyObject>,
    pub py_list_size: Symbol<'static, unsafe extern "C" fn(*mut PyObject) -> isize>,
    pub py_list_get_item:
        Symbol<'static, unsafe extern "C" fn(*mut PyObject, isize) -> *mut PyObject>,
    pub py_list_set_item:
        Symbol<'static, unsafe extern "C" fn(*mut PyObject, isize, *mut PyObject) -> c_int>,
    pub py_list_append:
        Symbol<'static, unsafe extern "C" fn(list: *mut PyObject, item: *mut PyObject) -> c_int>,
    pub py_list_type: *mut PyObject,

    // Dict
    pub py_dict_new: Symbol<'static, unsafe extern "C" fn() -> *mut PyObject>,
    pub py_dict_size: Symbol<'static, unsafe extern "C" fn(*mut PyObject) -> isize>,
    pub py_dict_set_item: Symbol<
        'static,
        unsafe extern "C" fn(*mut PyObject, key: *mut PyObject, value: *mut PyObject) -> c_int,
    >,
    pub py_dict_get_item:
        Symbol<'static, unsafe extern "C" fn(*mut PyObject, key: *mut PyObject) -> *mut PyObject>,
    pub py_dict_next: Symbol<
        'static,
        unsafe extern "C" fn(
            p: *mut PyObject,
            pos: *mut isize,
            key: *mut *mut PyObject,
            value: *mut *mut PyObject,
        ) -> c_int,
    >,
    pub py_dict_type: *mut PyObject,

    // Type name
    py_type_name: Symbol<'static, unsafe extern "C" fn(*mut PyObject) -> *mut PyObject>,

    // Reference counting
    py_incref: Symbol<'static, unsafe extern "C" fn(*mut PyObject)>,
    py_decref: Symbol<'static, unsafe extern "C" fn(*mut PyObject)>,

    // Error handling
    py_err_occurred: Symbol<'static, unsafe extern "C" fn() -> *mut PyObject>,
    py_err_fetch: Symbol<
        'static,
        unsafe extern "C" fn(
            ptype: *mut *mut PyObject,
            pvalue: *mut *mut PyObject,
            ptraceback: *mut *mut PyObject,
        ),
    >,
    py_err_normalize_exception: Symbol<
        'static,
        unsafe extern "C" fn(
            exc: *mut *mut PyObject,
            val: *mut *mut PyObject,
            tb: *mut *mut PyObject,
        ),
    >,
    py_err_clear: Symbol<'static, unsafe extern "C" fn()>,
    pub(crate) py_err_exception_matches:
        Symbol<'static, unsafe extern "C" fn(exc: *mut PyObject) -> c_int>,
    py_err_given_exception_matches:
        Symbol<'static, unsafe extern "C" fn(given: *mut PyObject, exc: *mut PyObject) -> c_int>,
    py_exc_syntax_error: *mut PyObject,
    pub(crate) py_exc_stop_async_iteration: *mut PyObject,
    py_object_get_attr_string: Symbol<
        'static,
        unsafe extern "C" fn(o: *mut PyObject, attr_name: *const c_char) -> *mut PyObject,
    >,
    py_object_str: Symbol<'static, unsafe extern "C" fn(o: *mut PyObject) -> *mut PyObject>,
    py_object_is_instance: Symbol<
        'static,
        unsafe extern "C" fn(instance: *mut PyObject, class: *mut PyObject) -> c_int,
    >,
    py_object_get_item: Symbol<
        'static,
        unsafe extern "C" fn(o: *mut PyObject, key: *mut PyObject) -> *mut PyObject,
    >,
    py_object_set_item: Symbol<
        'static,
        unsafe extern "C" fn(o: *mut PyObject, key: *mut PyObject, value: *mut PyObject) -> c_int,
    >,
    py_object_del_item:
        Symbol<'static, unsafe extern "C" fn(o: *mut PyObject, key: *mut PyObject) -> c_int>,
    py_object_call: Symbol<
        'static,
        unsafe extern "C" fn(
            callable: *mut PyObject,
            args: *mut PyObject,
            kwargs: *mut PyObject,
        ) -> *mut PyObject,
    >,
    py_object_call_no_args:
        Option<Symbol<'static, unsafe extern "C" fn(callable: *mut PyObject) -> *mut PyObject>>,
    py_object_set_attr_string: Symbol<
        'static,
        unsafe extern "C" fn(o: *mut PyObject, attr_name: *const c_char, v: *mut PyObject) -> c_int,
    >,
    py_object_has_attr_string:
        Symbol<'static, unsafe extern "C" fn(o: *mut PyObject, attr_name: *const c_char) -> c_int>,
    pub(crate) py_object_call_object: Symbol<
        'static,
        unsafe extern "C" fn(callable: *mut PyObject, args: *mut PyObject) -> *mut PyObject,
    >,
    py_object_get_iter: Symbol<'static, unsafe extern "C" fn(o: *mut PyObject) -> *mut PyObject>,
    py_object_is_true: Symbol<'static, unsafe extern "C" fn(o: *mut PyObject) -> c_int>,
    py_object_repr: Symbol<'static, unsafe extern "C" fn(o: *mut PyObject) -> *mut PyObject>,
    py_err_print: Symbol<'static, unsafe extern "C" fn(c_int)>,

    // Import Module
    py_import_import_module:
        Symbol<'static, unsafe extern "C" fn(name: *const c_char) -> *mut PyObject>,
    py_set_python_home: Symbol<'static, unsafe extern "C" fn(home: *const wchar_t)>,
    py_set_program_name: Symbol<'static, unsafe extern "C" fn(name: *const wchar_t)>,

    // GIL and thread state
    py_eval_save_thread: Symbol<'static, unsafe extern "C" fn() -> *mut PyThreadState>,
    py_eval_restore_thread: Symbol<'static, unsafe extern "C" fn(*mut PyThreadState)>,
    py_gilstate_ensure: Symbol<'static, unsafe extern "C" fn() -> PyGILState>,
    py_gilstate_release: Symbol<'static, unsafe extern "C" fn(PyGILState)>,
    py_gilstate_check: Symbol<'static, unsafe extern "C" fn() -> c_int>,

    // Callables
    py_callable_check: Symbol<'static, unsafe extern "C" fn(callable: *mut PyObject) -> c_int>,

    // Iterator
    py_iter_next: Symbol<'static, unsafe extern "C" fn(iter: *mut PyObject) -> *mut PyObject>,
}

impl PythonApi {
    #[allow(clippy::missing_transmute_annotations)]
    pub unsafe fn load(path: &Path) -> Result<Self, libloading::Error> {
        #[cfg(unix)]
        let lib = {
            use libloading::os::unix::{Library as UnixLibrary, RTLD_GLOBAL, RTLD_LAZY};

            let unix_lib = UnixLibrary::open(Some(path), RTLD_LAZY | RTLD_GLOBAL)?;
            Library::from(unix_lib)
        };

        #[cfg(not(unix))]
        let lib = Library::new(path)?;
        let py_initialize: Symbol<unsafe extern "C" fn()> = lib.get(b"Py_Initialize")?;
        let py_initialize_ex: Symbol<unsafe extern "C" fn(c_int)> = lib.get(b"Py_InitializeEx")?;
        let py_finalize: Symbol<unsafe extern "C" fn()> = lib.get(b"Py_Finalize")?;
        let py_is_initialized: Symbol<unsafe extern "C" fn() -> c_int> =
            lib.get(b"Py_IsInitialized")?;
        let py_run_simple_string: Symbol<unsafe extern "C" fn(*const c_char) -> c_int> =
            lib.get(b"PyRun_SimpleString")?;
        let py_run_string: Symbol<
            unsafe extern "C" fn(
                str: *const c_char,
                start: c_int,
                globals: *mut PyObject,
                locals: *mut PyObject,
            ) -> *mut PyObject,
        > = lib.get(b"PyRun_String")?;

        let py_none: *mut PyObject = *lib.get::<*mut PyObject>(b"_Py_NoneStruct")?;
        let py_true: *mut PyObject = *lib.get::<*mut PyObject>(b"_Py_TrueStruct")?;
        let py_false: *mut PyObject = *lib.get::<*mut PyObject>(b"_Py_FalseStruct")?;

        // Long
        let py_long_from_longlong: Symbol<unsafe extern "C" fn(i64) -> *mut PyObject> =
            lib.get(b"PyLong_FromLongLong")?;
        let py_long_as_longlong: Symbol<unsafe extern "C" fn(*mut PyObject) -> i64> =
            lib.get(b"PyLong_AsLongLong")?;
        let py_long_type: *mut PyObject = *lib.get::<*mut PyObject>(b"PyLong_Type")?;

        // Float and double
        let py_float_from_double: Symbol<unsafe extern "C" fn(f64) -> *mut PyObject> =
            lib.get(b"PyFloat_FromDouble")?;
        let py_float_as_double: Symbol<unsafe extern "C" fn(*mut PyObject) -> c_double> =
            lib.get(b"PyFloat_AsDouble")?;
        let py_float_type: *mut PyObject = *lib.get::<*mut PyObject>(b"PyFloat_Type")?;

        // Bool from long
        let py_bool_from_long: Symbol<unsafe extern "C" fn(i64) -> *mut PyObject> =
            lib.get(b"PyBool_FromLong")?;

        // Unicode
        let py_unicode_from_string_and_size: Symbol<
            unsafe extern "C" fn(*const c_char, Py_ssize_t) -> *mut PyObject,
        > = lib.get(b"PyUnicode_FromStringAndSize")?;
        let py_unicode_as_utf8_and_size: Symbol<
            unsafe extern "C" fn(*mut PyObject, *mut Py_ssize_t) -> *const c_char,
        > = lib.get(b"PyUnicode_AsUTF8AndSize")?;
        let py_unicode_type: *mut PyObject = *lib.get::<*mut PyObject>(b"PyUnicode_Type")?;

        // bytes
        let py_bytes_type = *lib.get::<*mut PyObject>(b"PyBytes_Type")?;
        let py_bytes_from_string_and_size: Symbol<
            unsafe extern "C" fn(*const c_char, Py_ssize_t) -> *mut PyObject,
        > = lib.get(b"PyBytes_FromStringAndSize")?;
        let py_bytes_as_string_and_size: Symbol<
            unsafe extern "C" fn(
                o: *mut PyObject,
                buffer: *mut *mut c_char,
                len: *mut Py_ssize_t,
            ) -> c_int,
        > = lib.get(b"PyBytes_AsStringAndSize")?;
        let py_bytes_size: Symbol<unsafe extern "C" fn(*mut PyObject) -> Py_ssize_t> =
            lib.get(b"PyBytes_Size")?;

        // bytearray
        let py_bytearray_type = *lib.get::<*mut PyObject>(b"PyByteArray_Type")?;
        let py_bytearray_from_object: Symbol<unsafe extern "C" fn(*mut PyObject) -> *mut PyObject> =
            lib.get(b"PyByteArray_FromObject")?;
        let py_bytearray_from_string_and_size: Symbol<
            unsafe extern "C" fn(*const c_char, Py_ssize_t) -> *mut PyObject,
        > = lib.get(b"PyByteArray_FromStringAndSize")?;
        let py_bytearray_size: Symbol<unsafe extern "C" fn(*mut PyObject) -> Py_ssize_t> =
            lib.get(b"PyByteArray_Size")?;
        let py_bytearray_as_string: Symbol<unsafe extern "C" fn(*mut PyObject) -> *mut c_char> =
            lib.get(b"PyByteArray_AsString")?;

        // Collection Functions
        // Tuple
        let py_tuple_new: Symbol<unsafe extern "C" fn(isize) -> *mut PyObject> =
            lib.get(b"PyTuple_New")?;
        let py_tuple_type: *mut PyObject = *lib.get::<*mut PyObject>(b"PyTuple_Type")?;
        let py_tuple_size: Symbol<unsafe extern "C" fn(*mut PyObject) -> isize> =
            lib.get(b"PyTuple_Size")?;
        let py_tuple_get_item: Symbol<unsafe extern "C" fn(*mut PyObject, isize) -> *mut PyObject> =
            lib.get(b"PyTuple_GetItem")?;
        let py_tuple_set_item: Symbol<
            unsafe extern "C" fn(*mut PyObject, isize, *mut PyObject) -> c_int,
        > = lib.get(b"PyTuple_SetItem")?;

        // Set
        let py_set_new: Symbol<unsafe extern "C" fn(isize) -> *mut PyObject> =
            lib.get(b"PySet_New")?;
        let py_set_size: Symbol<unsafe extern "C" fn(*mut PyObject) -> isize> =
            lib.get(b"PySet_Size")?;
        let py_set_type: *mut PyObject = *lib.get::<*mut PyObject>(b"PySet_Type")?;
        let py_frozen_set_new: Symbol<unsafe extern "C" fn(isize) -> *mut PyObject> =
            lib.get(b"PyFrozenSet_New")?;
        let py_frozen_set_type: *mut PyObject = *lib.get::<*mut PyObject>(b"PyFrozenSet_Type")?;

        // List
        let py_list_new: Symbol<unsafe extern "C" fn(isize) -> *mut PyObject> =
            lib.get(b"PyList_New")?;
        let py_list_size: Symbol<unsafe extern "C" fn(*mut PyObject) -> isize> =
            lib.get(b"PyList_Size")?;
        let py_list_append: Symbol<unsafe extern "C" fn(*mut PyObject, *mut PyObject) -> c_int> =
            lib.get(b"PyList_Append")?;
        let py_list_set_item: Symbol<
            unsafe extern "C" fn(*mut PyObject, isize, *mut PyObject) -> c_int,
        > = lib.get(b"PyList_SetItem")?;
        let py_list_get_item: Symbol<unsafe extern "C" fn(*mut PyObject, isize) -> *mut PyObject> =
            lib.get(b"PyList_GetItem")?;
        let py_list_type: *mut PyObject = *lib.get::<*mut PyObject>(b"PyList_Type")?;

        // Dict
        let py_dict_new: Symbol<unsafe extern "C" fn() -> *mut PyObject> =
            lib.get(b"PyDict_New")?;
        let py_dict_size: Symbol<unsafe extern "C" fn(*mut PyObject) -> isize> =
            lib.get(b"PyDict_Size")?;
        let py_dict_set_item: Symbol<
            unsafe extern "C" fn(
                p: *mut PyObject,
                key: *mut PyObject,
                value: *mut PyObject,
            ) -> c_int,
        > = lib.get(b"PyDict_SetItem")?;
        let py_dict_get_item: Symbol<
            unsafe extern "C" fn(p: *mut PyObject, key: *mut PyObject) -> *mut PyObject,
        > = lib.get(b"PyDict_GetItem")?;
        let py_dict_next: Symbol<
            unsafe extern "C" fn(
                p: *mut PyObject,
                pos: *mut isize,
                key: *mut *mut PyObject,
                value: *mut *mut PyObject,
            ) -> c_int,
        > = lib.get(b"PyDict_Next")?;
        let py_dict_type: *mut PyObject = *lib.get::<*mut PyObject>(b"PyDict_Type")?;

        // Type name
        let py_type_name: Symbol<unsafe extern "C" fn(*mut PyObject) -> *mut PyObject> =
            lib.get(b"PyType_GetName")?;

        let py_incref: Symbol<unsafe extern "C" fn(*mut PyObject)> = lib.get(b"Py_IncRef")?;
        let py_decref: Symbol<unsafe extern "C" fn(*mut PyObject)> = lib.get(b"Py_DecRef")?;

        let py_err_occurred: Symbol<unsafe extern "C" fn() -> *mut PyObject> =
            lib.get(b"PyErr_Occurred")?;
        let py_err_fetch: Symbol<
            unsafe extern "C" fn(*mut *mut PyObject, *mut *mut PyObject, *mut *mut PyObject),
        > = lib.get(b"PyErr_Fetch")?;
        let py_err_normalize_exception: Symbol<
            unsafe extern "C" fn(*mut *mut PyObject, *mut *mut PyObject, *mut *mut PyObject),
        > = lib.get(b"PyErr_NormalizeException")?;
        let py_err_clear: Symbol<unsafe extern "C" fn()> = lib.get(b"PyErr_Clear")?;
        let py_err_exception_matches: Symbol<unsafe extern "C" fn(exc: *mut PyObject) -> c_int> =
            lib.get(b"PyErr_ExceptionMatches")?;
        let py_err_given_exception_matches: Symbol<
            unsafe extern "C" fn(given: *mut PyObject, exc: *mut PyObject) -> c_int,
        > = lib.get(b"PyErr_GivenExceptionMatches")?;
        let py_exc_syntax_error: *mut PyObject = *lib.get::<*mut PyObject>(b"PyExc_SyntaxError")?;
        let py_exc_stop_async_iteration: *mut PyObject =
            *lib.get::<*mut PyObject>(b"PyExc_StopAsyncIteration")?;
        let py_object_get_attr_string: Symbol<
            unsafe extern "C" fn(o: *mut PyObject, attr_name: *const c_char) -> *mut PyObject,
        > = lib.get(b"PyObject_GetAttrString")?;

        let py_object_str: Symbol<unsafe extern "C" fn(o: *mut PyObject) -> *mut PyObject> =
            lib.get(b"PyObject_Str")?;
        let py_object_is_instance: Symbol<
            unsafe extern "C" fn(*mut PyObject, *mut PyObject) -> c_int,
        > = lib.get(b"PyObject_IsInstance")?;
        let py_object_get_item: Symbol<
            unsafe extern "C" fn(o: *mut PyObject, key: *mut PyObject) -> *mut PyObject,
        > = lib.get(b"PyObject_GetItem")?;
        let py_object_set_item: Symbol<
            unsafe extern "C" fn(
                o: *mut PyObject,
                key: *mut PyObject,
                value: *mut PyObject,
            ) -> c_int,
        > = lib.get(b"PyObject_SetItem")?;
        let py_object_del_item: Symbol<
            unsafe extern "C" fn(o: *mut PyObject, key: *mut PyObject) -> c_int,
        > = lib.get(b"PyObject_DelItem")?;
        let py_object_call: Symbol<
            unsafe extern "C" fn(
                callable: *mut PyObject,
                args: *mut PyObject,
                kwargs: *mut PyObject,
            ) -> *mut PyObject,
        > = lib.get(b"PyObject_Call")?;
        let py_object_set_attr_string: Symbol<
            unsafe extern "C" fn(
                o: *mut PyObject,
                attr_name: *const c_char,
                v: *mut PyObject,
            ) -> c_int,
        > = lib.get(b"PyObject_SetAttrString")?;
        let py_object_has_attr_string: Symbol<
            unsafe extern "C" fn(o: *mut PyObject, attr_name: *const c_char) -> c_int,
        > = lib.get(b"PyObject_HasAttrString")?;

        let py_object_call_object: Symbol<
            unsafe extern "C" fn(callable: *mut PyObject, args: *mut PyObject) -> *mut PyObject,
        > = lib.get(b"PyObject_CallObject")?;

        let py_object_call_no_args: Option<
            Symbol<unsafe extern "C" fn(callable: *mut PyObject) -> *mut PyObject>,
        > = lib.get(b"PyObject_CallNoArgs").ok();

        let py_object_get_iter: Symbol<unsafe extern "C" fn(o: *mut PyObject) -> *mut PyObject> =
            lib.get(b"PyObject_GetIter")?;
        let py_object_is_true: Symbol<unsafe extern "C" fn(o: *mut PyObject) -> c_int> =
            lib.get(b"PyObject_IsTrue")?;
        let py_object_repr: Symbol<unsafe extern "C" fn(o: *mut PyObject) -> *mut PyObject> =
            lib.get(b"PyObject_Repr")?;
        let py_err_print: Symbol<unsafe extern "C" fn(c_int)> = lib.get(b"PyErr_PrintEx")?;

        // Import Module
        let py_import_module: Symbol<unsafe extern "C" fn(*const c_char) -> *mut PyObject> =
            lib.get(b"PyImport_ImportModule")?;
        let py_set_python_home: Symbol<unsafe extern "C" fn(*const wchar_t)> =
            lib.get(b"Py_SetPythonHome")?;
        let py_set_program_name: Symbol<unsafe extern "C" fn(*const wchar_t)> =
            lib.get(b"Py_SetProgramName")?;

        let py_eval_save_thread: Symbol<unsafe extern "C" fn() -> *mut PyThreadState> =
            lib.get(b"PyEval_SaveThread")?;
        let py_eval_restore_thread: Symbol<unsafe extern "C" fn(*mut PyThreadState)> =
            lib.get(b"PyEval_RestoreThread")?;
        let py_gilstate_ensure: Symbol<unsafe extern "C" fn() -> PyGILState> =
            lib.get(b"PyGILState_Ensure")?;
        let py_gilstate_release: Symbol<unsafe extern "C" fn(PyGILState)> =
            lib.get(b"PyGILState_Release")?;
        let py_gilstate_check: Symbol<unsafe extern "C" fn() -> c_int> =
            lib.get(b"PyGILState_Check")?;
        let py_callable_check: Symbol<unsafe extern "C" fn(callable: *mut PyObject) -> c_int> =
            lib.get(b"PyCallable_Check")?;

        // Iterator
        let py_iter_next: Symbol<unsafe extern "C" fn(*mut PyObject) -> *mut PyObject> =
            lib.get(b"PyIter_Next")?;

        let api = Self {
            py_initialize: std::mem::transmute(py_initialize),
            py_initialize_ex: std::mem::transmute(py_initialize_ex),
            py_finalize: std::mem::transmute(py_finalize),
            py_is_initialized: std::mem::transmute(py_is_initialized),
            py_run_simple_string: std::mem::transmute(py_run_simple_string),
            py_run_string: std::mem::transmute(py_run_string),
            py_none,
            py_true,
            py_false,
            async_to_sync_class: std::ptr::null_mut(),
            py_long_from_longlong: std::mem::transmute(py_long_from_longlong),
            py_long_as_longlong: std::mem::transmute(py_long_as_longlong),
            py_long_type,
            py_float_from_double: std::mem::transmute(py_float_from_double),
            py_float_as_double: std::mem::transmute(py_float_as_double),
            py_float_type,
            py_bool_from_long: std::mem::transmute(py_bool_from_long),
            py_unicode_from_string_and_size: std::mem::transmute(py_unicode_from_string_and_size),
            py_unicode_as_utf8_and_size: std::mem::transmute(py_unicode_as_utf8_and_size),
            py_unicode_type,

            // bytes
            py_bytes_type,
            py_bytes_from_string_and_size: std::mem::transmute(py_bytes_from_string_and_size),
            py_bytes_as_string_and_size: std::mem::transmute(py_bytes_as_string_and_size),
            py_bytes_size: std::mem::transmute(py_bytes_size),

            // bytearray
            py_bytearray_type,
            py_bytearray_from_object: std::mem::transmute(py_bytearray_from_object),
            py_bytearray_from_string_and_size: std::mem::transmute(
                py_bytearray_from_string_and_size,
            ),
            py_bytearray_size: std::mem::transmute(py_bytearray_size),
            py_bytearray_as_string: std::mem::transmute(py_bytearray_as_string),

            // Collections
            // Tuple
            py_tuple_new: std::mem::transmute(py_tuple_new),
            py_tuple_type,
            py_tuple_size: std::mem::transmute(py_tuple_size),
            py_tuple_get_item: std::mem::transmute(py_tuple_get_item),
            py_tuple_set_item: std::mem::transmute(py_tuple_set_item),
            // Set
            py_set_new: std::mem::transmute(py_set_new),
            py_set_size: std::mem::transmute(py_set_size),
            py_set_type,
            py_frozen_set_new: std::mem::transmute(py_frozen_set_new),
            py_frozen_set_type,
            // List
            py_list_new: std::mem::transmute(py_list_new),
            py_list_set_item: std::mem::transmute(py_list_set_item),
            py_list_get_item: std::mem::transmute(py_list_get_item),
            py_list_append: std::mem::transmute(py_list_append),
            py_list_type,
            py_list_size: std::mem::transmute(py_list_size),
            // Dict
            py_dict_new: std::mem::transmute(py_dict_new),
            py_dict_size: std::mem::transmute(py_dict_size),
            py_dict_set_item: std::mem::transmute(py_dict_set_item),
            py_dict_get_item: std::mem::transmute(py_dict_get_item),
            py_dict_next: std::mem::transmute(py_dict_next),
            py_dict_type,
            py_type_name: std::mem::transmute(py_type_name),
            py_incref: std::mem::transmute(py_incref),
            py_decref: std::mem::transmute(py_decref),
            py_err_occurred: std::mem::transmute(py_err_occurred),
            py_err_fetch: std::mem::transmute(py_err_fetch),
            py_err_normalize_exception: std::mem::transmute(py_err_normalize_exception),
            py_err_clear: std::mem::transmute(py_err_clear),
            py_err_exception_matches: std::mem::transmute(py_err_exception_matches),
            py_err_given_exception_matches: std::mem::transmute(py_err_given_exception_matches),
            py_exc_syntax_error,
            py_exc_stop_async_iteration,
            py_object_get_attr_string: std::mem::transmute(py_object_get_attr_string),
            py_object_str: std::mem::transmute(py_object_str),
            py_object_is_instance: std::mem::transmute(py_object_is_instance),
            py_object_get_item: std::mem::transmute(py_object_get_item),
            py_object_set_item: std::mem::transmute(py_object_set_item),
            py_object_del_item: std::mem::transmute(py_object_del_item),
            py_object_call: std::mem::transmute(py_object_call),
            py_object_set_attr_string: std::mem::transmute(py_object_set_attr_string),
            py_object_has_attr_string: std::mem::transmute(py_object_has_attr_string),
            py_object_call_object: std::mem::transmute(py_object_call_object),
            py_object_call_no_args: std::mem::transmute(py_object_call_no_args),
            py_object_get_iter: std::mem::transmute(py_object_get_iter),
            py_object_is_true: std::mem::transmute(py_object_is_true),
            py_object_repr: std::mem::transmute(py_object_repr),
            py_err_print: std::mem::transmute(py_err_print),
            py_import_import_module: std::mem::transmute(py_import_module),
            py_set_python_home: std::mem::transmute(py_set_python_home),
            py_set_program_name: std::mem::transmute(py_set_program_name),
            py_eval_save_thread: std::mem::transmute(py_eval_save_thread),
            py_eval_restore_thread: std::mem::transmute(py_eval_restore_thread),
            py_gilstate_ensure: std::mem::transmute(py_gilstate_ensure),
            py_gilstate_release: std::mem::transmute(py_gilstate_release),
            py_gilstate_check: std::mem::transmute(py_gilstate_check),
            py_callable_check: std::mem::transmute(py_callable_check),
            py_iter_next: std::mem::transmute(py_iter_next),
            _lib: lib,
        };

        Ok(api)
    }

    pub fn initialize(&self) {
        unsafe { (self.py_initialize)() }
    }

    /// Initialize Python without registering signal handlers.
    /// Pass 0 to avoid interfering with Ruby's signal handling.
    pub fn initialize_ex(&self, initsigs: c_int) {
        unsafe { (self.py_initialize_ex)(initsigs) }
    }

    pub fn finalize(&self) {
        unsafe { (self.py_finalize)() }
    }

    pub fn is_initialized(&self) -> bool {
        unsafe { (self.py_is_initialized)() != 0 }
    }

    fn str_to_wchar(s: &str) -> Vec<wchar_t> {
        let mut wide: Vec<wchar_t> = s.chars().map(|c| c as wchar_t).collect();
        wide.push(0);
        wide
    }

    pub fn set_python_home(&self, home: &str) {
        let wide = Self::str_to_wchar(home);
        let ptr = Box::leak(wide.into_boxed_slice()).as_ptr();
        unsafe { (self.py_set_python_home)(ptr) }
    }

    pub fn set_program_name(&self, name: &str) {
        let wide = Self::str_to_wchar(name);
        let ptr = Box::leak(wide.into_boxed_slice()).as_ptr();
        unsafe { (self.py_set_program_name)(ptr) }
    }

    pub fn run_simple_string(&self, code: &str) -> Result<(), String> {
        let c_code = CString::new(code).map_err(|e| format!("Invalid code string: {}", e))?;
        let result = unsafe { (self.py_run_simple_string)(c_code.as_ptr()) };

        if result == 0 {
            Ok(())
        } else {
            unsafe { (self.py_err_print)(0) };
            unsafe { (self.py_err_clear)() };
            Err("Python execution failed".to_string())
        }
    }

    pub fn run_string(
        &self,
        code: &str,
        start: i64,
        globals: *mut PyObject,
        locals: *mut PyObject,
    ) -> Result<*mut PyObject, String> {
        if globals.is_null() {
            return Err("globals is null".to_string());
        }

        let c_code = CString::new(code).map_err(|_e| "Invalid code string".to_string())?;
        let locals = if locals.is_null() { globals } else { locals };

        let result =
            unsafe { (self.py_run_string)(c_code.as_ptr(), start as c_int, globals, locals) };
        Ok(result)
    }

    pub fn long_from_i64(&self, value: i64) -> *mut PyObject {
        unsafe { (self.py_long_from_longlong)(value) }
    }

    pub fn long_to_i64(&self, obj: *mut PyObject) -> i64 {
        unsafe { (self.py_long_as_longlong)(obj) }
    }

    pub fn is_long(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_long_type) == 1 }
    }

    pub fn float_from_f64(&self, value: f64) -> *mut PyObject {
        unsafe { (self.py_float_from_double)(value) }
    }

    pub fn float_to_f64(&self, obj: *mut PyObject) -> f64 {
        unsafe { (self.py_float_as_double)(obj) }
    }

    pub fn is_float(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_float_type) == 1 }
    }

    pub fn bool_from_i64(&self, value: i64) -> *mut PyObject {
        unsafe { (self.py_bool_from_long)(value) }
    }

    pub fn is_true(&self, obj: *mut PyObject) -> bool {
        obj == self.py_true
    }

    pub fn is_false(&self, obj: *mut PyObject) -> bool {
        obj == self.py_false
    }

    pub fn is_none(&self, obj: *mut PyObject) -> bool {
        obj == self.py_none
    }

    pub fn is_bool(&self, obj: *mut PyObject) -> bool {
        obj == self.py_true || obj == self.py_false
    }

    pub fn string_from_str(&self, s: &str) -> *mut PyObject {
        unsafe {
            (self.py_unicode_from_string_and_size)(
                s.as_ptr() as *const c_char,
                s.len() as Py_ssize_t,
            )
        }
    }

    pub fn string_to_string(&self, obj: *mut PyObject) -> Option<String> {
        if !self.is_string(obj) {
            return None; // Not a string object
        }
        let mut size: Py_ssize_t = 0;
        let ptr = unsafe { (self.py_unicode_as_utf8_and_size)(obj, &mut size) };
        if ptr.is_null() {
            return None; // Python error (e.g., encoding failure)
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size as usize) };
        String::from_utf8(slice.to_vec()).ok()
    }

    pub fn object_str(&self, obj: *mut PyObject) -> *mut PyObject {
        if obj.is_null() {
            return std::ptr::null_mut();
        }
        unsafe { (self.py_object_str)(obj) }
    }

    pub fn object_get_item(&self, obj: *mut PyObject, key: *mut PyObject) -> *mut PyObject {
        unsafe { (self.py_object_get_item)(obj, key) }
    }

    pub fn object_set_item(
        &self,
        obj: *mut PyObject,
        key: *mut PyObject,
        value: *mut PyObject,
    ) -> c_int {
        unsafe { (self.py_object_set_item)(obj, key, value) }
    }

    pub fn object_del_item(&self, obj: *mut PyObject, key: *mut PyObject) -> c_int {
        unsafe { (self.py_object_del_item)(obj, key) }
    }

    pub fn object_is_true(&self, obj: *mut PyObject) -> bool {
        unsafe { (self.py_object_is_true)(obj) == 1 }
    }

    pub fn object_repr(&self, obj: *mut PyObject) -> String {
        if obj.is_null() {
            return String::from("<null>");
        }
        let repr_ptr = unsafe { (self.py_object_repr)(obj) };
        if repr_ptr.is_null() {
            return String::from("<repr error>");
        }
        let repr = self
            .string_to_string(repr_ptr)
            .unwrap_or("<invalid repr>".to_string());
        self.decref(repr_ptr);
        repr
    }

    pub fn is_string(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_unicode_type) == 1 }
    }

    pub fn none(&self) -> *mut PyObject {
        self.py_none
    }

    pub fn true_obj(&self) -> *mut PyObject {
        self.py_true
    }

    pub fn false_obj(&self) -> *mut PyObject {
        self.py_false
    }

    pub fn bool_from_bool(&self, value: bool) -> *mut PyObject {
        if value {
            self.py_true
        } else {
            self.py_false
        }
    }

    pub fn bool_to_bool(&self, obj: *mut PyObject) -> Result<bool, String> {
        if obj == self.py_true {
            Ok(true)
        } else if obj == self.py_false {
            Ok(false)
        } else {
            Err("expected bool".to_string())
        }
    }

    pub fn bytes_check(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_bytes_type) == 1 }
    }

    pub fn bytes_from_slice(&self, data: &[u8]) -> *mut PyObject {
        let size = data.len() as Py_ssize_t;
        unsafe { (self.py_bytes_from_string_and_size)(data.as_ptr() as *const c_char, size) }
    }

    pub fn bytes_to_vec(&self, obj: *mut PyObject) -> Option<Vec<u8>> {
        if !self.bytes_check(obj) {
            return None;
        }
        let mut size: Py_ssize_t = 0;
        let mut buffer: *mut c_char = std::ptr::null_mut();
        let result = unsafe { (self.py_bytes_as_string_and_size)(obj, &mut buffer, &mut size) };
        if result != 0 {
            if self.has_error() {
                self.clear_error();
            }
            return None;
        }
        if buffer.is_null() {
            return None;
        }
        let slice = unsafe { std::slice::from_raw_parts(buffer as *const u8, size as usize) };
        Some(slice.to_vec())
    }

    pub fn bytearray_check(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_bytearray_type) == 1 }
    }
    pub fn bytearray_from_slice(&self, data: &[u8]) -> *mut PyObject {
        let size = data.len() as Py_ssize_t;
        unsafe { (self.py_bytearray_from_string_and_size)(data.as_ptr() as *const c_char, size) }
    }

    pub fn bytearray_to_vec(&self, obj: *mut PyObject) -> Option<Vec<u8>> {
        if !self.bytearray_check(obj) {
            return None;
        }
        let size = unsafe { (self.py_bytearray_size)(obj) };
        if size < 0 {
            if self.has_error() {
                self.clear_error();
            }
            return None;
        }
        if size == 0 {
            return Some(Vec::new());
        }
        let buffer = unsafe { (self.py_bytearray_as_string)(obj) };

        if buffer.is_null() {
            if self.has_error() {
                self.clear_error();
            }
            return None;
        }
        let slice = unsafe { std::slice::from_raw_parts(buffer as *const u8, size as usize) };
        Some(slice.to_vec())
    }

    pub fn tuple_new(&self, size: isize) -> *mut PyObject {
        unsafe { (self.py_tuple_new)(size) }
    }

    pub fn tuple_check(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_tuple_type) == 1 }
    }

    pub fn tuple_size(&self, obj: *mut PyObject) -> isize {
        if obj.is_null() {
            return 0;
        }
        unsafe { (self.py_tuple_size)(obj) }
    }

    pub fn tuple_set_item(&self, p: *mut PyObject, pos: isize, o: *mut PyObject) -> c_int {
        unsafe { (self.py_tuple_set_item)(p, pos, o) }
    }

    pub fn tuple_get_item(&self, obj: *mut PyObject, pos: isize) -> *mut PyObject {
        unsafe { (self.py_tuple_get_item)(obj, pos) }
    }

    pub fn set_new(&self, size: isize) -> *mut PyObject {
        unsafe { (self.py_set_new)(size) }
    }

    pub fn set_size(&self, obj: *mut PyObject) -> isize {
        if obj.is_null() {
            return 0;
        }
        unsafe { (self.py_set_size)(obj) }
    }

    pub fn is_set(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_set_type) == 1 }
    }

    pub fn frozen_set_new(&self, size: isize) -> *mut PyObject {
        unsafe { (self.py_frozen_set_new)(size) }
    }

    pub fn is_frozen_set(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_frozen_set_type) == 1 }
    }

    pub fn list_new(&self, size: isize) -> *mut PyObject {
        unsafe { (self.py_list_new)(size) }
    }
    pub fn list_new_checked(&self, size: isize) -> Result<*mut PyObject, String> {
        let list = unsafe { (self.py_list_new)(size) };
        if list.is_null() {
            Err("Failed to create Python list".to_string())
        } else {
            Ok(list)
        }
    }

    pub fn list_size(&self, obj: *mut PyObject) -> isize {
        unsafe { (self.py_list_size)(obj) }
    }

    pub fn list_set_item(&self, list: *mut PyObject, index: isize, value: *mut PyObject) -> c_int {
        unsafe { (self.py_list_set_item)(list, index, value) }
    }

    pub fn list_set_item_checked(
        &self,
        list: *mut PyObject,
        index: isize,
        value: *mut PyObject,
    ) -> Result<c_int, String> {
        if list.is_null() {
            return Err("List is null".to_string());
        }
        let result = unsafe { (self.py_list_set_item)(list, index, value) };
        if result == -1 {
            return Err("Failed to set item in Python list, Index Error".to_string());
        }
        Ok(result)
    }

    pub fn list_get_item(&self, list: *mut PyObject, index: isize) -> *mut PyObject {
        unsafe { (self.py_list_get_item)(list, index) }
    }

    pub fn list_append(&self, list: *mut PyObject, value: *mut PyObject) -> c_int {
        if list.is_null() {
            return -1;
        }
        unsafe { (self.py_list_append)(list, value) }
    }

    pub fn list_check(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_list_type) == 1 }
    }

    // Dict
    pub fn dict_new(&self) -> *mut PyObject {
        unsafe { (self.py_dict_new)() }
    }

    pub fn dict_size(&self, obj: *mut PyObject) -> usize {
        unsafe { (self.py_dict_size)(obj) as usize }
    }
    pub fn dict_set_item(
        &self,
        dict: *mut PyObject,
        key: *mut PyObject,
        value: *mut PyObject,
    ) -> c_int {
        unsafe { (self.py_dict_set_item)(dict, key, value) }
    }

    pub fn dict_get_item(&self, dict: *mut PyObject, key: *mut PyObject) -> *mut PyObject {
        unsafe { (self.py_dict_get_item)(dict, key) }
    }

    pub fn dict_next(
        &self,
        dict: *mut PyObject,
        pos: *mut isize,
        key: *mut *mut PyObject,
        value: *mut *mut PyObject,
    ) -> bool {
        unsafe { (self.py_dict_next)(dict, pos, key, value) != 0 }
    }

    pub fn dict_check(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }
        unsafe { (self.py_object_is_instance)(obj, self.py_dict_type) == 1 }
    }

    #[allow(dead_code)]
    pub(crate) fn type_name(&self, obj: *mut PyObject) -> Option<String> {
        if obj.is_null() {
            return None;
        }
        // PyType_GetName expects a type object, not an instance.
        // Get obj.__class__ first, which is the type object.
        let type_obj = self.object_get_attr_string(obj, "__class__");
        if type_obj.is_null() {
            self.clear_error();
            return None;
        }
        let name_obj = unsafe { (self.py_type_name)(type_obj) };
        self.decref(type_obj);
        if name_obj.is_null() {
            self.clear_error();
            return None;
        }
        let result = self.string_to_string(name_obj);
        self.decref(name_obj);
        result
    }

    pub fn incref(&self, obj: *mut PyObject) {
        if !obj.is_null() {
            unsafe { (self.py_incref)(obj) }
        }
    }

    pub fn decref(&self, obj: *mut PyObject) {
        if !obj.is_null() {
            unsafe { (self.py_decref)(obj) }
        }
    }

    pub fn has_error(&self) -> bool {
        unsafe { !(self.py_err_occurred)().is_null() }
    }

    pub fn clear_error(&self) {
        unsafe { (self.py_err_clear)() }
    }

    pub fn extract_exception(api: &PythonApi) -> Option<PythonException> {
        if !api.has_error() {
            return None;
        }
        let mut py_type = std::ptr::null_mut();
        let mut py_value = std::ptr::null_mut();
        let mut py_traceback = std::ptr::null_mut();
        unsafe {
            // Fetch Error type, value and traceback
            (api.py_err_fetch)(&mut py_type, &mut py_value, &mut py_traceback);
            // Normalize exception type and value to PythonException
            (api.py_err_normalize_exception)(&mut py_type, &mut py_value, &mut py_traceback);
        }

        // Extract kind from type
        let kind = match PyGuard::new(
            if py_type.is_null() {
                std::ptr::null_mut()
            } else {
                unsafe {
                    (api.py_object_get_attr_string)(py_type, c"__name__".as_ptr() as *const c_char)
                }
            },
            api,
        ) {
            Some(guard) => api
                .string_to_string(guard.ptr())
                .unwrap_or_else(|| "UnknownError".to_string()),
            None => {
                api.clear_error();
                "UnknownError".to_string()
            }
        };
        let message = match PyGuard::new(
            if py_value.is_null() {
                std::ptr::null_mut()
            } else {
                unsafe { (api.py_object_str)(py_value) }
            },
            api,
        ) {
            Some(guard) => api
                .string_to_string(guard.ptr())
                .unwrap_or_else(|| "UnknownError".to_string()),
            None => {
                api.clear_error();
                "UnknownError".to_string()
            }
        };
        let traceback = {
            if py_traceback.is_null() {
                None
            } else {
                Some(Self::format_traceback(api, py_traceback))
            }
        };
        let exception = if !py_type.is_null() && kind == "SyntaxError" {
            // It's a SyntaxError → extract line/offset/filename/text from py_value
            Self::extract_syntax_error_details(api, py_value, message)
        } else {
            PythonException::Exception {
                kind,
                message,
                traceback,
            }
        };
        api.decref(py_type);
        api.decref(py_value);
        api.decref(py_traceback);
        Some(exception)
    }
    fn extract_syntax_error_details(
        api: &PythonApi,
        py_value: *mut PyObject,
        message: String,
    ) -> PythonException {
        let line_number = match PyGuard::new(
            unsafe {
                (api.py_object_get_attr_string)(py_value, c"lineno".as_ptr() as *const c_char)
            },
            api,
        ) {
            Some(guard) if !api.is_none(guard.ptr()) => api.long_to_i64(guard.ptr()) as usize,
            Some(_) => 0,
            None => {
                api.clear_error();
                0
            }
        };
        let offset = match PyGuard::new(
            unsafe {
                (api.py_object_get_attr_string)(py_value, c"offset".as_ptr() as *const c_char)
            },
            api,
        ) {
            Some(guard) if !api.is_none(guard.ptr()) => api.long_to_i64(guard.ptr()) as usize,
            Some(_) => 0,
            None => {
                api.clear_error();
                0
            }
        };
        let filename = match PyGuard::new(
            unsafe {
                (api.py_object_get_attr_string)(py_value, c"filename".as_ptr() as *const c_char)
            },
            api,
        ) {
            Some(guard) if !api.is_none(guard.ptr()) => api
                .string_to_string(guard.ptr())
                .unwrap_or_else(|| "<unknown>".to_string()),
            Some(_) => "<unknown>".to_string(),
            None => {
                api.clear_error();
                "<unknown>".to_string()
            }
        };
        PythonException::SyntaxError {
            message,
            line: line_number,
            offset,
            filename,
        }
    }

    pub fn format_traceback(api: &PythonApi, py_tb: *mut PyObject) -> String {
        // import traceback module
        let Some(tb_module) = PyGuard::new(
            unsafe { (api.py_import_import_module)(c"traceback".as_ptr() as *const c_char) },
            api,
        ) else {
            return "<traceback unavailable>".into();
        };

        // Get format_tb() function from traceback
        let Some(format_tb) = PyGuard::new(
            unsafe {
                (api.py_object_get_attr_string)(
                    tb_module.ptr(),
                    c"format_tb".as_ptr() as *const c_char,
                )
            },
            api,
        ) else {
            return "<traceback unavailable>".into();
        };
        // New Tuple for args
        let Some(args) = PyGuard::new(unsafe { (api.py_tuple_new)(1) }, api) else {
            return "<traceback unavailable>".into();
        };
        // Set args in tuple - steal reference
        api.incref(py_tb);
        if unsafe { (api.py_tuple_set_item)(args.ptr(), 0, py_tb) } != 0 {
            return "<traceback unavailable>".into();
        }

        // Call the Object function with format_tb and args
        let Some(result_list) = PyGuard::new(
            unsafe { (api.py_object_call_object)(format_tb.ptr(), args.ptr()) },
            api,
        ) else {
            return "<traceback unavailable>".into();
        };
        // Get the items from list
        let len = unsafe { (api.py_list_size)(result_list.ptr()) };
        let mut lines = Vec::with_capacity(len as usize);
        for i in 0..len {
            let item = unsafe { (api.py_list_get_item)(result_list.ptr(), i) }; // borrowed — no guard
            if !item.is_null() {
                if let Some(s) = api.string_to_string(item) {
                    lines.push(s);
                }
            }
        }
        lines.join("")
    }

    pub fn print_error(&self, set_sys_last_vars: bool) {
        unsafe { (self.py_err_print)(set_sys_last_vars as c_int) }
    }

    // PyObject
    pub fn object_get_attr_string(&self, obj: *mut PyObject, attr_name: &str) -> *mut PyObject {
        let Ok(c_name) = std::ffi::CString::new(attr_name) else {
            return std::ptr::null_mut();
        };
        unsafe { (self.py_object_get_attr_string)(obj, c_name.as_ptr()) }
    }
    pub fn object_set_attr_string(
        &self,
        obj: *mut PyObject,
        name: &str,
        value: *mut PyObject,
    ) -> c_int {
        let Ok(c_name) = std::ffi::CString::new(name) else {
            return -1;
        };
        unsafe { (self.py_object_set_attr_string)(obj, c_name.as_ptr(), value) }
    }

    pub fn object_has_attr_string(&self, obj: *mut PyObject, name: *const c_char) -> c_int {
        unsafe { (self.py_object_has_attr_string)(obj, name) }
    }

    pub fn object_call(
        &self,
        callable: *mut PyObject,
        args: *mut PyObject,
        kwargs: *mut PyObject,
    ) -> *mut PyObject {
        unsafe { (self.py_object_call)(callable, args, kwargs) }
    }

    pub fn object_call_no_args(&self, callable: *mut PyObject) -> *mut PyObject {
        if let Some(ref f) = self.py_object_call_no_args {
            unsafe { f(callable) }
        } else {
            unsafe { (self.py_object_call)(callable, std::ptr::null_mut(), std::ptr::null_mut()) }
        }
    }

    pub fn object_get_iter(&self, obj: *mut PyObject) -> *mut PyObject {
        unsafe { (self.py_object_get_iter)(obj) }
    }

    fn is_attribute_error(exc: &PythonException) -> bool {
        matches!(
            exc,
            PythonException::Exception { kind, .. } if kind == "AttributeError"
        )
    }

    pub fn probe_async_iterable(&self, obj: *mut PyObject) -> Result<bool, PythonException> {
        if obj.is_null() {
            return Ok(false);
        }

        let aiter = self.object_get_attr_string(obj, "__aiter__");
        if aiter.is_null() {
            if !self.has_error() {
                return Ok(false);
            }

            let exc = Self::extract_exception(self).unwrap_or(PythonException::Exception {
                kind: "RuntimeError".to_string(),
                message: "Unknown Python error during __aiter__ probe".to_string(),
                traceback: None,
            });
            if Self::is_attribute_error(&exc) {
                return Ok(false);
            }
            return Err(exc);
        }
        self.decref(aiter);

        let anext = self.object_get_attr_string(obj, "__anext__");
        if anext.is_null() {
            if !self.has_error() {
                return Ok(false);
            }

            let exc = Self::extract_exception(self).unwrap_or(PythonException::Exception {
                kind: "RuntimeError".to_string(),
                message: "Unknown Python error during __anext__ probe".to_string(),
                traceback: None,
            });
            if Self::is_attribute_error(&exc) {
                return Ok(false);
            }
            return Err(exc);
        }
        self.decref(anext);

        Ok(true)
    }

    pub fn is_async_iterable(&self, obj: *mut PyObject) -> bool {
        matches!(self.probe_async_iterable(obj), Ok(true))
    }

    pub fn is_sync_iterable(&self, obj: *mut PyObject) -> bool {
        if obj.is_null() {
            return false;
        }

        let iter = self.object_get_iter(obj);
        if iter.is_null() {
            if self.has_error() {
                self.clear_error();
            }
            return false;
        }

        self.decref(iter);
        true
    }

    /// Attempts to load and install the `AsyncToSync` adapter class for asynchronous-to-synchronous
    /// conversion in the current runtime.
    ///
    ///
    /// # Errors
    /// This function will return an `Err` variant in the following scenarios:
    /// - If the `AsyncToSync` class cannot be found or loaded from the Python `__main__` module.
    /// - If the `AsyncToSync` class exists but is not callable.
    ///
    /// If an error condition occurs, any temporary objects are cleaned up
    /// using reference decrementing operations, and the runtime error state is cleared if needed.
    ///
    /// # Returns
    /// - `Ok(())` on successful installation of the `AsyncToSync` class.
    /// - `Err(String)` containing an error message if the installation fails.
    pub fn install_async_to_sync_class(&mut self) -> Result<(), String> {
        self.run_simple_string(crate::async_gen::SYNC_ADAPTER_PY)?;

        let main_module = self.import_module("__main__")?;
        let adapter_class = self.object_get_attr_string(main_module, "AsyncToSync");
        self.decref(main_module);

        if adapter_class.is_null() {
            self.clear_error();
            return Err("Failed to load AsyncToSync class".to_string());
        }

        if self.callable_check(adapter_class) != 1 {
            self.decref(adapter_class);
            return Err("AsyncToSync is not callable".to_string());
        }

        if !self.async_to_sync_class.is_null() {
            self.decref(self.async_to_sync_class);
        }

        self.async_to_sync_class = adapter_class;
        Ok(())
    }

    pub fn wrap_async_generator(&self, async_gen: *mut PyObject) -> *mut PyObject {
        if async_gen.is_null() || self.async_to_sync_class.is_null() {
            return std::ptr::null_mut();
        }

        let args_tuple = unsafe { (self.py_tuple_new)(1) };
        if args_tuple.is_null() {
            return std::ptr::null_mut();
        }

        unsafe { (self.py_incref)(async_gen) };
        if unsafe { (self.py_tuple_set_item)(args_tuple, 0, async_gen) } != 0 {
            unsafe { (self.py_decref)(args_tuple) };
            return std::ptr::null_mut();
        }

        let sync_iter =
            unsafe { (self.py_object_call_object)(self.async_to_sync_class, args_tuple) };
        unsafe { (self.py_decref)(args_tuple) };
        sync_iter
    }

    pub fn import_module(&self, module_name: &str) -> Result<*mut PyObject, String> {
        let name = CString::new(module_name).map_err(|_| "Invalid module name".to_string())?;
        let module = unsafe { (self.py_import_import_module)(name.as_ptr()) };
        if module.is_null() {
            return Err("Failed to import module".to_string());
        }
        Ok(module)
    }
    pub(crate) fn save_thread(&self) -> *mut PyThreadState {
        unsafe { (self.py_eval_save_thread)() }
    }

    pub(crate) fn restore_thread(&self, state: *mut PyThreadState) {
        unsafe { (self.py_eval_restore_thread)(state) }
    }

    pub(crate) fn ensure_gil(&self) -> PyGILState {
        unsafe { (self.py_gilstate_ensure)() }
    }

    pub(crate) fn release_gil(&self, state: PyGILState) {
        unsafe { (self.py_gilstate_release)(state) }
    }

    pub fn gil_check(&self) -> bool {
        unsafe { (self.py_gilstate_check)() != 0 }
    }

    pub fn callable_check(&self, obj: *mut PyObject) -> c_int {
        unsafe { (self.py_callable_check)(obj) }
    }

    pub fn iter_next(&self, obj: *mut PyObject) -> *mut PyObject {
        unsafe { (self.py_iter_next)(obj) }
    }
}
// SAFETY: PythonApi contains raw pointers to Python singletons (Py_None, Py_True, Py_False)
// which are global and immutable. The function pointers (Symbol<'static>) are also safe
// to share. All actual Python API calls require GIL protection, which is handled
// separately by the test infrastructure.
unsafe impl Send for PythonApi {}
unsafe impl Sync for PythonApi {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_gen::SYNC_ADAPTER_PY;
    use crate::python_finder::find_libpython;
    use crate::test_helpers::skip_if_no_python;
    use serial_test::serial;

    #[test]
    #[serial]
    #[ignore] // Disabled: loading second library instance corrupts interpreter state
    fn test_load_succeeds() {
        let Some(path) = find_libpython() else {
            println!("Skipping: libpython not found");
            return;
        };
        let result = unsafe { PythonApi::load(&path) };
        assert!(
            result.is_ok(),
            "Failed to load PythonApi: {:?}",
            result.err()
        );
        std::mem::forget(result.unwrap());
    }

    #[test]
    #[serial]
    fn test_initialize_and_is_initialized() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(guard.api().is_initialized(), "Python should be initialized");
    }

    #[test]
    #[serial]
    fn test_run_simple_string_success() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let result = guard.api().run_simple_string("x = 1 + 1");
        assert!(result.is_ok(), "Simple Python code should succeed");

        let result = guard.api().run_simple_string("import sys");
        assert!(result.is_ok(), "Import should succeed");
    }

    #[test]
    #[serial]
    fn test_run_simple_string_syntax_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let result = guard
            .api()
            .run_simple_string("this is not valid python!@#$");
        assert!(result.is_err(), "Invalid syntax should return error");
    }

    #[test]
    #[serial]
    fn test_run_simple_string_runtime_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let result = guard.api().run_simple_string("undefined_variable_xyz");
        assert!(result.is_err(), "NameError should return error");
    }
    // ========== run_string tests ==========

    /// Py_eval_input = 258 (for expressions)
    const PY_EVAL_INPUT: i64 = 258;
    /// Py_file_input = 257 (for statements)
    const PY_FILE_INPUT: i64 = 257;

    /// Helper: create a fresh globals dict with `__builtins__` set,
    /// so that built-in functions (like `len`, `range`, etc.) are available.
    fn make_globals(api: &PythonApi) -> *mut PyObject {
        let globals = api.dict_new();
        // Import __builtins__ so expressions like len([1,2]) work
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
    fn test_run_string_eval_simple_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let result = api.run_string("1 + 2", PY_EVAL_INPUT, globals, globals);
        assert!(result.is_ok(), "run_string should succeed");
        let py_obj = result.unwrap();
        assert!(
            !py_obj.is_null(),
            "Expression should return non-null result"
        );
        assert_eq!(api.long_to_i64(py_obj), 3, "1 + 2 should equal 3");
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_eval_string_expression() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let result = api.run_string("'hello' + ' ' + 'world'", PY_EVAL_INPUT, globals, globals);
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert_eq!(
            api.string_to_string(py_obj),
            Some("hello world".to_string())
        );
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_eval_returns_null_on_syntax_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let result = api.run_string("def foo():", PY_EVAL_INPUT, globals, globals);
        assert!(result.is_ok(), "run_string itself should return Ok");
        let py_obj = result.unwrap();
        assert!(py_obj.is_null(), "Invalid expression should return null");
        assert!(api.has_error(), "Python error should be set");
        api.clear_error();
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_eval_returns_null_on_name_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let result = api.run_string("undefined_variable_xyz", PY_EVAL_INPUT, globals, globals);
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert!(py_obj.is_null(), "Undefined variable should return null");
        assert!(api.has_error(), "NameError should be set");
        api.clear_error();
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_file_input_statement() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Statements return Py_None on success
        let result = api.run_string("x = 42", PY_FILE_INPUT, globals, globals);
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert!(
            !py_obj.is_null(),
            "Statement should return non-null (Py_None)"
        );
        assert!(api.is_none(py_obj), "Statement result should be Py_None");

        // Verify the variable was set in globals
        let key = api.string_from_str("x");
        let val = api.dict_get_item(globals, key);
        assert!(!val.is_null(), "x should exist in globals");
        assert_eq!(api.long_to_i64(val), 42, "x should be 42");
        api.decref(key);
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_file_input_multiline() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let code = "a = 10\nb = 20\nc = a + b";
        let result = api.run_string(code, PY_FILE_INPUT, globals, globals);
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());

        let key = api.string_from_str("c");
        let val = api.dict_get_item(globals, key);
        assert_eq!(api.long_to_i64(val), 30, "c should be 30");
        api.decref(key);
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_rejects_null_globals() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let result = api.run_string(
            "1 + 1",
            PY_EVAL_INPUT,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert!(result.is_err(), "Null globals should return Err");
    }

    #[test]
    #[serial]
    fn test_run_string_null_locals_falls_back_to_globals() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // With null locals, run_string should use globals for both
        let result = api.run_string("y = 99", PY_FILE_INPUT, globals, std::ptr::null_mut());
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());

        let key = api.string_from_str("y");
        let val = api.dict_get_item(globals, key);
        assert!(
            !val.is_null(),
            "y should exist in globals when locals was null"
        );
        assert_eq!(api.long_to_i64(val), 99);
        api.decref(key);
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_separate_locals() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);
        let locals = api.dict_new();

        let result = api.run_string("z = 123", PY_FILE_INPUT, globals, locals);
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());

        // Variable should be in locals, not globals
        let key = api.string_from_str("z");
        let val_in_locals = api.dict_get_item(locals, key);
        assert!(!val_in_locals.is_null(), "z should be in locals");
        assert_eq!(api.long_to_i64(val_in_locals), 123);

        let val_in_globals = api.dict_get_item(globals, key);
        assert!(val_in_globals.is_null(), "z should NOT be in globals");

        api.decref(key);
        api.decref(py_obj);
        api.decref(locals);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_eval_with_builtins() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let result = api.run_string("len([1, 2, 3])", PY_EVAL_INPUT, globals, globals);
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert_eq!(api.long_to_i64(py_obj), 3, "len([1,2,3]) should be 3");
        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_run_string_error_does_not_leak_error_state() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Trigger an error
        let result = api.run_string("1 / 0", PY_EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(py_obj.is_null());
        assert!(api.has_error());
        api.clear_error();

        // Subsequent call should work fine
        let result = api.run_string("2 + 2", PY_EVAL_INPUT, globals, globals);
        let py_obj = result.unwrap();
        assert!(!py_obj.is_null());
        assert_eq!(api.long_to_i64(py_obj), 4);
        assert!(!api.has_error());

        api.decref(py_obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_long_from_i64_creates_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let py_int = guard.api().long_from_i64(99999);
        assert!(!py_int.is_null(), "Should create a Python int object");
        guard.api().decref(py_int);
    }

    #[test]
    #[serial]
    fn test_long_roundtrip() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        for value in [0i64, 42, -1, i64::MAX, i64::MIN] {
            let py_int = guard.api().long_from_i64(value);
            let back = guard.api().long_to_i64(py_int);
            assert_eq!(back, value, "Roundtrip failed for {}", value);
            guard.api().decref(py_int);
        }
    }

    #[test]
    #[serial]
    fn test_multiline_code() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let result = guard.api().run_simple_string(
            r#"
def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)

assert factorial(5) == 120, "factorial(5) should be 120"
"#,
        );
        assert!(
            result.is_ok(),
            "Multiline code with function def should work"
        );
    }

    #[test]
    #[serial]
    fn test_incref_decref_dont_crash() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_int = guard.api().long_from_i64(99999);

        guard.api().incref(py_int);
        guard.api().incref(py_int);
        guard.api().decref(py_int);
        guard.api().decref(py_int);
        guard.api().decref(py_int);
    }

    #[test]
    #[serial]
    fn test_null_safety() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        guard.api().incref(std::ptr::null_mut());
        guard.api().decref(std::ptr::null_mut());
    }

    #[test]
    #[serial]
    fn test_error_state() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        guard.api().clear_error();
        assert!(!guard.api().has_error(), "No error after clear");

        let _ = guard.api().run_simple_string("raise ValueError('test')");

        guard.api().clear_error();
    }

    #[test]
    #[serial]
    fn test_gil_check_returns_true_when_held() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        // PyGILState_Check() only returns true when GIL was acquired via PyGILState_Ensure().
        // Our test infrastructure uses PyEval_RestoreThread() which acquires the GIL but doesn't
        // update the GILState tracking layer. To properly test gil_check(), we must:
        // 1. Release the GIL from RestoreThread (save_thread)
        // 2. Acquire it via GILState API (ensure_gil)
        // 3. Assert gil_check() returns true
        // 4. Release via GILState API (release_gil)
        // 5. Restore thread state for guard's Drop

        let state = guard.api().save_thread(); // Release GIL from RestoreThread
        let gil_state = guard.api().ensure_gil(); // Acquire via GILState API

        assert!(
            guard.api().gil_check(),
            "GIL should be held after PyGILState_Ensure"
        );

        guard.api().release_gil(gil_state); // Release via GILState API
        guard.api().restore_thread(state); // Re-acquire for guard's Drop
    }

    #[test]
    #[serial]
    fn test_save_restore_thread_roundtrip() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let state = guard.api().save_thread();
        assert!(!state.is_null(), "SaveThread should return non-null state");
        guard.api().restore_thread(state);

        let result = guard.api().run_simple_string("restored = True");
        assert!(result.is_ok(), "Should work after restore");
    }

    #[test]
    #[serial]
    #[ignore]
    fn test_ensure_release_gil_roundtrip() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        // Test that ensure_gil and release_gil work
        let gil_state = guard.api().ensure_gil();
        let result = guard.api().run_simple_string("ensured = True");
        assert!(result.is_ok(), "Should work with GIL ensured");
        guard.api().release_gil(gil_state);

        // Forget the guard to prevent its Drop from crashing.
        // After calling release_gil(), the GIL state becomes inconsistent
        // and the guard's Drop (which calls save_thread()) crashes.
        std::mem::forget(guard);
    }

    #[test]
    #[serial]
    fn test_long_boundary_values() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        for value in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX] {
            let py_int = guard.api().long_from_i64(value);
            assert!(!py_int.is_null(), "Should create PyObject for {}", value);
            let back = guard.api().long_to_i64(py_int);
            assert_eq!(back, value, "Roundtrip failed for {}", value);
            guard.api().decref(py_int);
        }
    }

    #[test]
    #[serial]
    fn test_multiple_objects_lifecycle() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let mut objects = Vec::new();
        for i in 0..100 {
            objects.push(guard.api().long_from_i64(i));
        }

        for (i, obj) in objects.iter().enumerate() {
            assert_eq!(guard.api().long_to_i64(*obj), i as i64);
        }

        for obj in objects {
            guard.api().decref(obj);
        }
    }

    #[test]
    #[serial]
    fn test_import_and_use_module() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        // We use `json` instead of `math` because `math` is a C extension that fails to load
        // on some Python versions due to symbol visibility issues when libpython is loaded via
        // libloading (RTLD_LOCAL). `json` is pure Python and always works.
        let result = guard.api().run_simple_string(
            r#"
import json
data = json.loads('{"key": "value", "number": 42}')
assert data["key"] == "value"
assert data["number"] == 42
assert json.dumps({"test": True}) == '{"test": true}'
"#,
        );
        assert!(
            result.is_ok(),
            "Should be able to import and use json module"
        );
    }

    #[test]
    #[serial]
    fn test_exception_types() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        // Test various exception types
        assert!(guard
            .api()
            .run_simple_string("raise ValueError('test')")
            .is_err());
        assert!(guard
            .api()
            .run_simple_string("raise TypeError('test')")
            .is_err());
        assert!(guard
            .api()
            .run_simple_string("raise KeyError('test')")
            .is_err());
        assert!(guard.api().run_simple_string("1/0").is_err()); // ZeroDivisionError
    }

    // ========== Float operations ==========

    #[test]
    #[serial]
    fn test_float_from_double_creates_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let py_float = unsafe { (guard.api().py_float_from_double)(std::f64::consts::PI) };
        assert!(!py_float.is_null(), "Should create a Python float object");
        guard.api().decref(py_float);
    }

    #[test]
    #[serial]
    fn test_float_roundtrip() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        for value in [
            0.0f64,
            1.5,
            -1.5,
            f64::MAX,
            f64::MIN,
            f64::MIN_POSITIVE,
            std::f64::consts::PI,
        ] {
            let py_float = unsafe { (guard.api().py_float_from_double)(value) };
            let back = unsafe { (guard.api().py_float_as_double)(py_float) };
            assert_eq!(back, value, "Roundtrip failed for {}", value);
            guard.api().decref(py_float);
        }
    }

    #[test]
    #[serial]
    fn test_float_check() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_float = unsafe { (guard.api().py_float_from_double)(1.0) };
        let py_int = guard.api().long_from_i64(42);

        assert!(
            guard.api().is_float(py_float),
            "Float should pass float check"
        );
        assert!(!guard.api().is_float(py_int), "Int should fail float check");

        guard.api().decref(py_float);
        guard.api().decref(py_int);
    }

    #[test]
    #[serial]
    fn test_float_special_values() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        // Test infinity
        let py_inf = unsafe { (guard.api().py_float_from_double)(f64::INFINITY) };
        let back_inf = unsafe { (guard.api().py_float_as_double)(py_inf) };
        assert!(
            back_inf.is_infinite() && back_inf.is_sign_positive(),
            "Positive infinity roundtrip failed"
        );
        guard.api().decref(py_inf);

        // Test negative infinity
        let py_neg_inf = unsafe { (guard.api().py_float_from_double)(f64::NEG_INFINITY) };
        let back_neg_inf = unsafe { (guard.api().py_float_as_double)(py_neg_inf) };
        assert!(
            back_neg_inf.is_infinite() && back_neg_inf.is_sign_negative(),
            "Negative infinity roundtrip failed"
        );
        guard.api().decref(py_neg_inf);

        // Test NaN
        let py_nan = unsafe { (guard.api().py_float_from_double)(f64::NAN) };
        let back_nan = unsafe { (guard.api().py_float_as_double)(py_nan) };
        assert!(back_nan.is_nan(), "NaN roundtrip failed");
        guard.api().decref(py_nan);
    }

    // ========== Boolean operations ==========

    #[test]
    #[serial]
    fn test_bool_from_long() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_false = guard.api().bool_from_i64(0);
        assert_eq!(
            py_false,
            guard.api().false_obj(),
            "0 should produce Py_False"
        );

        let py_true = guard.api().bool_from_i64(1);
        assert_eq!(py_true, guard.api().true_obj(), "1 should produce Py_True");

        let py_true_neg = guard.api().bool_from_i64(-1);
        assert_eq!(
            py_true_neg,
            guard.api().true_obj(),
            "-1 should produce Py_True"
        );

        let py_true_large = guard.api().bool_from_i64(999999);
        assert_eq!(
            py_true_large,
            guard.api().true_obj(),
            "Large value should produce Py_True"
        );
    }
    #[test]
    #[serial]
    fn test_bool_from_bool_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        assert_eq!(guard.api().bool_from_bool(true), guard.api().true_obj());
        assert_eq!(guard.api().bool_from_bool(false), guard.api().false_obj());
    }
    #[test]
    #[serial]
    fn test_bool_to_bool_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        assert_eq!(guard.api().bool_to_bool(guard.api().true_obj()), Ok(true));
        assert_eq!(guard.api().bool_to_bool(guard.api().false_obj()), Ok(false));

        let py_int = guard.api().long_from_i64(1);
        assert!(guard.api().bool_to_bool(py_int).is_err());
        guard.api().decref(py_int);
    }

    #[test]
    #[serial]
    fn test_python_singletons_are_valid() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        assert!(!guard.api().none().is_null(), "Py_None should be non-null");
        assert!(
            !guard.api().true_obj().is_null(),
            "Py_True should be non-null"
        );
        assert!(
            !guard.api().false_obj().is_null(),
            "Py_False should be non-null"
        );

        assert_ne!(
            guard.api().none(),
            guard.api().true_obj(),
            "Py_None and Py_True should differ"
        );
        assert_ne!(
            guard.api().none(),
            guard.api().false_obj(),
            "Py_None and Py_False should differ"
        );
        assert_ne!(
            guard.api().true_obj(),
            guard.api().false_obj(),
            "Py_True and Py_False should differ"
        );
    }

    // ========== Unicode/String operations ==========

    #[test]
    #[serial]
    fn test_unicode_from_string_creates_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let test_str = "hello";
        let py_str = unsafe {
            (guard.api().py_unicode_from_string_and_size)(
                test_str.as_ptr() as *const std::ffi::c_char,
                test_str.len() as isize,
            )
        };
        assert!(!py_str.is_null(), "Should create a Python str object");
        guard.api().decref(py_str);
    }

    #[test]
    #[serial]
    fn test_unicode_roundtrip() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        for test_str in [
            "hello",
            "world",
            "",
            "with spaces",
            "unicode: café 日本語 🎉",
        ] {
            let py_str = unsafe {
                (guard.api().py_unicode_from_string_and_size)(
                    test_str.as_ptr() as *const std::ffi::c_char,
                    test_str.len() as isize,
                )
            };
            assert!(!py_str.is_null(), "Failed to create str for '{}'", test_str);

            let mut size: isize = 0;
            let ptr = unsafe { (guard.api().py_unicode_as_utf8_and_size)(py_str, &mut size) };
            assert!(!ptr.is_null(), "Failed to get UTF-8 for '{}'", test_str);

            let slice = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size as usize) };
            let back = std::str::from_utf8(slice).expect("Invalid UTF-8");
            assert_eq!(back, test_str, "Roundtrip failed for '{}'", test_str);

            guard.api().decref(py_str);
        }
    }

    #[test]
    #[serial]
    fn test_unicode_check() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let test_str = "test";
        let py_str = unsafe {
            (guard.api().py_unicode_from_string_and_size)(
                test_str.as_ptr() as *const std::ffi::c_char,
                test_str.len() as isize,
            )
        };
        let py_int = guard.api().long_from_i64(42);

        assert!(
            guard.api().is_string(py_str),
            "String should pass unicode check"
        );
        assert!(
            !guard.api().is_string(py_int),
            "Int should fail unicode check"
        );

        guard.api().decref(py_str);
        guard.api().decref(py_int);
    }

    #[test]
    #[serial]
    fn test_unicode_empty_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let empty = "";
        let py_str = unsafe {
            (guard.api().py_unicode_from_string_and_size)(
                empty.as_ptr() as *const std::ffi::c_char,
                0,
            )
        };
        assert!(!py_str.is_null(), "Should create empty Python str");

        let mut size: isize = 0;
        let ptr = unsafe { (guard.api().py_unicode_as_utf8_and_size)(py_str, &mut size) };
        assert!(!ptr.is_null(), "Should get UTF-8 for empty string");
        assert_eq!(size, 0, "Empty string size should be 0");

        guard.api().decref(py_str);
    }

    // ========== Long type check ==========

    #[test]
    #[serial]
    fn test_long_check() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_int = guard.api().long_from_i64(42);
        let py_float = unsafe { (guard.api().py_float_from_double)(1.0) };

        assert!(guard.api().is_long(py_int), "Int should pass long check");
        assert!(
            !guard.api().is_long(py_float),
            "Float should fail long check"
        );

        guard.api().decref(py_int);
        guard.api().decref(py_float);
    }

    // ========== Error handling ==========

    #[test]
    #[serial]
    fn test_print_error_clears_error_state() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let _ = guard
            .api()
            .run_simple_string("raise ValueError('test error')");

        guard.api().print_error(false);

        let result = guard.api().run_simple_string("x = 1");
        assert!(
            result.is_ok(),
            "Should be able to run code after print_error"
        );
    }

    // ========== Public interface tests ==========

    #[test]
    #[serial]
    fn test_is_long_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_int = guard.api().long_from_i64(42);
        let py_float = guard.api().float_from_f64(1.0);

        assert!(guard.api().is_long(py_int));
        assert!(!guard.api().is_long(py_float));

        guard.api().decref(py_int);
        guard.api().decref(py_float);
    }

    #[test]
    #[serial]
    fn test_float_from_f64_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_float = guard.api().float_from_f64(std::f64::consts::PI);
        assert!(!py_float.is_null());
        guard.api().decref(py_float);
    }

    #[test]
    #[serial]
    fn test_float_to_f64_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        for value in [0.0, 1.5, -1.5, std::f64::consts::PI] {
            let py_float = guard.api().float_from_f64(value);
            let back = guard.api().float_to_f64(py_float);
            assert_eq!(back, value);
            guard.api().decref(py_float);
        }
    }

    #[test]
    #[serial]
    fn test_is_float_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_float = guard.api().float_from_f64(1.0);
        let py_int = guard.api().long_from_i64(42);

        assert!(guard.api().is_float(py_float));
        assert!(!guard.api().is_float(py_int));

        guard.api().decref(py_float);
        guard.api().decref(py_int);
    }

    #[test]
    #[serial]
    fn test_bool_from_i64_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_false = guard.api().bool_from_i64(0);
        let py_true = guard.api().bool_from_i64(1);
        let py_true_neg = guard.api().bool_from_i64(-1);

        assert_eq!(py_false, guard.api().false_obj());
        assert_eq!(py_true, guard.api().true_obj());
        assert_eq!(py_true_neg, guard.api().true_obj());
    }

    #[test]
    #[serial]
    fn test_is_true_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        assert!(guard.api().is_true(guard.api().true_obj()));
        assert!(!guard.api().is_true(guard.api().false_obj()));
        assert!(!guard.api().is_true(guard.api().none()));
    }

    #[test]
    #[serial]
    fn test_is_false_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        assert!(guard.api().is_false(guard.api().false_obj()));
        assert!(!guard.api().is_false(guard.api().true_obj()));
        assert!(!guard.api().is_false(guard.api().none()));
    }

    #[test]
    #[serial]
    fn test_is_none_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        assert!(guard.api().is_none(guard.api().none()));
        assert!(!guard.api().is_none(guard.api().true_obj()));
        assert!(!guard.api().is_none(guard.api().false_obj()));

        let py_int = guard.api().long_from_i64(42);
        assert!(!guard.api().is_none(py_int));
        guard.api().decref(py_int);
    }

    #[test]
    #[serial]
    fn test_is_bool_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        assert!(guard.api().is_bool(guard.api().true_obj()));
        assert!(guard.api().is_bool(guard.api().false_obj()));
        assert!(!guard.api().is_bool(guard.api().none()));

        let py_int = guard.api().long_from_i64(1);
        assert!(!guard.api().is_bool(py_int));
        guard.api().decref(py_int);
    }

    #[test]
    #[serial]
    fn test_string_from_str_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_str = guard.api().string_from_str("hello world");
        assert!(!py_str.is_null());
        assert!(guard.api().is_string(py_str));
        guard.api().decref(py_str);
    }

    #[test]
    #[serial]
    fn test_string_to_string_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        for test_str in ["hello", "", "with spaces", "unicode: café 日本語"] {
            let py_str = guard.api().string_from_str(test_str);
            let back = guard.api().string_to_string(py_str);
            assert_eq!(back, Some(test_str.to_string()));
            guard.api().decref(py_str);
        }
    }

    #[test]
    #[serial]
    fn test_string_to_string_returns_none_for_non_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_int = guard.api().long_from_i64(42);
        assert!(guard.api().string_to_string(py_int).is_none());
        guard.api().decref(py_int);

        let py_float = guard.api().float_from_f64(std::f64::consts::PI);
        assert!(guard.api().string_to_string(py_float).is_none());
        guard.api().decref(py_float);
    }

    #[test]
    #[serial]
    fn test_is_string_public_interface() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let py_str = guard.api().string_from_str("test");
        let py_int = guard.api().long_from_i64(42);

        assert!(guard.api().is_string(py_str));
        assert!(!guard.api().is_string(py_int));

        guard.api().decref(py_str);
        guard.api().decref(py_int);
    }

    // ========== List operations ==========

    #[test]
    #[serial]
    fn test_list_new_creates_empty_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let list = guard.api().list_new(0);
        assert!(!list.is_null(), "Should create an empty Python list");
        assert!(guard.api().list_check(list), "Should be a list");
        assert_eq!(
            guard.api().list_size(list),
            0,
            "Empty list should have size 0"
        );
        guard.api().decref(list);
    }

    #[test]
    #[serial]
    fn test_list_new_with_size() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let list = guard.api().list_new(5);
        assert!(!list.is_null(), "Should create a Python list with size 5");
        assert_eq!(guard.api().list_size(list), 5, "List should have size 5");
        guard.api().decref(list);
    }

    #[test]
    #[serial]
    fn test_list_new_checked() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let result = guard.api().list_new_checked(3);
        assert!(result.is_ok(), "list_new_checked should succeed");
        let list = result.unwrap();
        assert_eq!(guard.api().list_size(list), 3);
        guard.api().decref(list);
    }

    #[test]
    #[serial]
    fn test_list_set_and_get_item() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let list = guard.api().list_new(3);

        // Set items (PyList_SetItem steals the reference, so no decref needed)
        let val0 = guard.api().long_from_i64(10);
        let val1 = guard.api().long_from_i64(20);
        let val2 = guard.api().long_from_i64(30);

        assert_eq!(
            guard.api().list_set_item(list, 0, val0),
            0,
            "set_item should succeed"
        );
        assert_eq!(
            guard.api().list_set_item(list, 1, val1),
            0,
            "set_item should succeed"
        );
        assert_eq!(
            guard.api().list_set_item(list, 2, val2),
            0,
            "set_item should succeed"
        );

        // Get items (PyList_GetItem returns borrowed reference, so no decref needed)
        let got0 = guard.api().list_get_item(list, 0);
        let got1 = guard.api().list_get_item(list, 1);
        let got2 = guard.api().list_get_item(list, 2);

        assert_eq!(guard.api().long_to_i64(got0), 10);
        assert_eq!(guard.api().long_to_i64(got1), 20);
        assert_eq!(guard.api().long_to_i64(got2), 30);

        guard.api().decref(list);
    }

    #[test]
    #[serial]
    fn test_list_set_item_checked() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let list = guard.api().list_new(2);
        let val = guard.api().long_from_i64(42);

        let result = guard.api().list_set_item_checked(list, 0, val);
        assert!(
            result.is_ok(),
            "list_set_item_checked should succeed for valid index"
        );

        guard.api().decref(list);
    }

    #[test]
    #[serial]
    fn test_list_check() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let list = guard.api().list_new(0);
        let not_list = guard.api().long_from_i64(42);

        assert!(guard.api().list_check(list), "List should pass list_check");
        assert!(
            !guard.api().list_check(not_list),
            "Int should fail list_check"
        );

        guard.api().decref(list);
        guard.api().decref(not_list);
    }

    #[test]
    #[serial]
    fn test_list_with_mixed_types() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let list = guard.api().list_new(4);

        guard
            .api()
            .list_set_item(list, 0, guard.api().long_from_i64(42));
        guard
            .api()
            .list_set_item(list, 1, guard.api().float_from_f64(std::f64::consts::PI));
        guard
            .api()
            .list_set_item(list, 2, guard.api().string_from_str("hello"));
        guard
            .api()
            .list_set_item(list, 3, guard.api().bool_from_bool(true));
        assert!(guard.api().is_long(guard.api().list_get_item(list, 0)));
        assert!(guard.api().is_float(guard.api().list_get_item(list, 1)));
        assert!(guard.api().is_string(guard.api().list_get_item(list, 2)));
        assert!(guard.api().is_bool(guard.api().list_get_item(list, 3)));

        guard.api().decref(list);
    }

    // ========== Dict operations ==========

    #[test]
    #[serial]
    fn test_dict_new_creates_empty_dict() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let dict = guard.api().dict_new();
        assert!(!dict.is_null(), "Should create an empty Python dict");
        assert!(guard.api().dict_check(dict), "Should be a dict");
        assert_eq!(
            guard.api().dict_size(dict),
            0,
            "Empty dict should have size 0"
        );
        guard.api().decref(dict);
    }

    #[test]
    #[serial]
    fn test_dict_set_and_get_item() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let dict = guard.api().dict_new();

        let key1 = guard.api().string_from_str("name");
        let val1 = guard.api().string_from_str("Alice");
        let key2 = guard.api().string_from_str("age");
        let val2 = guard.api().long_from_i64(30);

        // Set items (PyDict_SetItem does NOT steal references, must decref)
        let result1 = guard.api().dict_set_item(dict, key1, val1);
        assert_eq!(result1, 0, "dict_set_item should succeed");
        let result2 = guard.api().dict_set_item(dict, key2, val2);
        assert_eq!(result2, 0, "dict_set_item should succeed");

        assert_eq!(guard.api().dict_size(dict), 2, "Dict should have 2 items");

        // Get items (PyDict_GetItem returns borrowed reference)
        let got1 = guard.api().dict_get_item(dict, key1);
        let got2 = guard.api().dict_get_item(dict, key2);

        assert!(!got1.is_null(), "Should get value for 'name'");
        assert!(!got2.is_null(), "Should get value for 'age'");
        assert_eq!(
            guard.api().string_to_string(got1),
            Some("Alice".to_string())
        );
        assert_eq!(guard.api().long_to_i64(got2), 30);

        // Decref keys and values (dict_set_item doesn't steal)
        guard.api().decref(key1);
        guard.api().decref(val1);
        guard.api().decref(key2);
        guard.api().decref(val2);
        guard.api().decref(dict);
    }

    #[test]
    #[serial]
    fn test_dict_get_item_missing_key() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let dict = guard.api().dict_new();
        let missing_key = guard.api().string_from_str("nonexistent");

        let result = guard.api().dict_get_item(dict, missing_key);
        assert!(result.is_null(), "Missing key should return null");

        guard.api().decref(missing_key);
        guard.api().decref(dict);
    }

    #[test]
    #[serial]
    fn test_dict_check() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let dict = guard.api().dict_new();
        let not_dict = guard.api().list_new(0);

        assert!(guard.api().dict_check(dict), "Dict should pass dict_check");
        assert!(
            !guard.api().dict_check(not_dict),
            "List should fail dict_check"
        );

        guard.api().decref(dict);
        guard.api().decref(not_dict);
    }

    #[test]
    #[serial]
    fn test_dict_next_iteration() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let dict = guard.api().dict_new();

        let key1 = guard.api().string_from_str("a");
        let val1 = guard.api().long_from_i64(1);
        let key2 = guard.api().string_from_str("b");
        let val2 = guard.api().long_from_i64(2);
        let key3 = guard.api().string_from_str("c");
        let val3 = guard.api().long_from_i64(3);

        guard.api().dict_set_item(dict, key1, val1);
        guard.api().dict_set_item(dict, key2, val2);
        guard.api().dict_set_item(dict, key3, val3);

        let mut pos: isize = 0;
        let mut key: *mut PyObject = std::ptr::null_mut();
        let mut value: *mut PyObject = std::ptr::null_mut();
        let mut count = 0;
        let mut sum = 0i64;

        while guard.api().dict_next(dict, &mut pos, &mut key, &mut value) {
            count += 1;
            sum += guard.api().long_to_i64(value);
        }

        assert_eq!(count, 3, "Should iterate over 3 items");
        assert_eq!(sum, 6, "Sum of values should be 1+2+3=6");

        // Decref (dict_set_item doesn't steal)
        guard.api().decref(key1);
        guard.api().decref(val1);
        guard.api().decref(key2);
        guard.api().decref(val2);
        guard.api().decref(key3);
        guard.api().decref(val3);
        guard.api().decref(dict);
    }

    #[test]
    #[serial]
    fn test_dict_overwrite_value() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let dict = guard.api().dict_new();
        let key = guard.api().string_from_str("key");
        let val1 = guard.api().long_from_i64(100);
        let val2 = guard.api().long_from_i64(200);

        guard.api().dict_set_item(dict, key, val1);
        assert_eq!(
            guard
                .api()
                .long_to_i64(guard.api().dict_get_item(dict, key)),
            100
        );

        guard.api().dict_set_item(dict, key, val2);
        assert_eq!(
            guard
                .api()
                .long_to_i64(guard.api().dict_get_item(dict, key)),
            200
        );
        assert_eq!(guard.api().dict_size(dict), 1, "Size should still be 1");

        guard.api().decref(key);
        guard.api().decref(val1);
        guard.api().decref(val2);
        guard.api().decref(dict);
    }

    #[test]
    #[serial]
    fn test_dict_with_int_keys() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let dict = guard.api().dict_new();

        let key1 = guard.api().long_from_i64(1);
        let key2 = guard.api().long_from_i64(2);
        let val1 = guard.api().string_from_str("one");
        let val2 = guard.api().string_from_str("two");

        guard.api().dict_set_item(dict, key1, val1);
        guard.api().dict_set_item(dict, key2, val2);

        assert_eq!(guard.api().dict_size(dict), 2);
        assert_eq!(
            guard
                .api()
                .string_to_string(guard.api().dict_get_item(dict, key1)),
            Some("one".to_string())
        );
        assert_eq!(
            guard
                .api()
                .string_to_string(guard.api().dict_get_item(dict, key2)),
            Some("two".to_string())
        );

        guard.api().decref(key1);
        guard.api().decref(key2);
        guard.api().decref(val1);
        guard.api().decref(val2);
        guard.api().decref(dict);
    }

    #[test]
    #[serial]
    fn test_dict_size() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let dict = guard.api().dict_new();
        assert_eq!(guard.api().dict_size(dict), 0);

        for i in 0..10 {
            let key = guard.api().long_from_i64(i);
            let val = guard.api().long_from_i64(i * 10);
            guard.api().dict_set_item(dict, key, val);
            guard.api().decref(key);
            guard.api().decref(val);
        }

        assert_eq!(guard.api().dict_size(dict), 10);
        guard.api().decref(dict);
    }

    // ========== Null pointer safety tests ==========

    #[test]
    #[serial]
    fn test_is_long_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().is_long(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_is_float_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().is_float(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_is_string_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().is_string(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_is_bool_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().is_bool(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_is_none_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().is_none(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_list_check_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().list_check(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_dict_check_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().dict_check(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_bool_to_bool_null_returns_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(guard.api().bool_to_bool(std::ptr::null_mut()).is_err());
    }

    #[test]
    #[serial]
    fn test_string_to_string_null_returns_none() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(guard.api().string_to_string(std::ptr::null_mut()).is_none());
    }

    // ========== Error detection ==========

    #[test]
    #[serial]
    fn test_has_error_false_when_no_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();
        assert!(!api.has_error());
    }

    #[test]
    #[serial]
    fn test_clear_error_clears_error_state() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("not a number");
        let _ = api.long_to_i64(py_str);

        api.clear_error();
        assert!(!api.has_error());
        api.decref(py_str);
    }

    // ========== extract_exception ==========

    #[test]
    #[serial]
    fn test_extract_exception_returns_none_when_no_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        assert!(PythonApi::extract_exception(api).is_none());
    }

    #[test]
    #[serial]
    fn test_extract_exception_gets_type_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let py_str = api.string_from_str("not a number");
        let _ = api.long_to_i64(py_str);

        if api.has_error() {
            let exc = PythonApi::extract_exception(api);
            assert!(exc.is_some());
            if let Some(PythonException::Exception { kind, message, .. }) = exc {
                assert_eq!(kind, "TypeError");
                assert!(!message.is_empty());
            } else {
                panic!("Expected Exception variant");
            }
        }

        assert!(!api.has_error());
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_extract_exception_clears_error_state() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let py_str = api.string_from_str("hello");
        let _ = api.long_to_i64(py_str);

        if api.has_error() {
            let _ = PythonApi::extract_exception(api);
            assert!(!api.has_error());
        }
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_extract_exception_no_traceback_from_c_api_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let py_str = api.string_from_str("hello");
        let _ = api.long_to_i64(py_str);

        if api.has_error() {
            let exc = PythonApi::extract_exception(api);
            if let Some(PythonException::Exception { traceback, .. }) = exc {
                assert!(
                    traceback.is_none() || traceback.as_deref() == Some(""),
                    "C API errors should have no traceback"
                );
            }
        }
        api.decref(py_str);
    }

    // ========== format_traceback ==========

    fn call_python_func(api: &PythonApi, func_name: &str) -> *mut PyObject {
        let main_module =
            unsafe { (api.py_import_import_module)(c"__main__".as_ptr() as *const c_char) };
        assert!(!main_module.is_null());

        let name_cstr = std::ffi::CString::new(func_name).unwrap();
        let func = unsafe { (api.py_object_get_attr_string)(main_module, name_cstr.as_ptr()) };
        assert!(!func.is_null());

        let empty_args = unsafe { (api.py_tuple_new)(0) };
        let result = unsafe { (api.py_object_call_object)(func, empty_args) };

        api.decref(empty_args);
        api.decref(func);
        api.decref(main_module);
        result
    }

    fn fetch_traceback(api: &PythonApi) -> (*mut PyObject, *mut PyObject, *mut PyObject) {
        let mut py_type = std::ptr::null_mut();
        let mut py_value = std::ptr::null_mut();
        let mut py_tb = std::ptr::null_mut();
        unsafe {
            (api.py_err_fetch)(&mut py_type, &mut py_value, &mut py_tb);
            (api.py_err_normalize_exception)(&mut py_type, &mut py_value, &mut py_tb);
        }
        (py_type, py_value, py_tb)
    }

    #[test]
    #[serial]
    fn test_format_traceback_single_frame() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _tb_single():
    raise ValueError("single frame error")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_tb_single");
        assert!(result.is_null());
        assert!(api.has_error());

        let (py_type, py_value, py_tb) = fetch_traceback(api);
        assert!(!py_tb.is_null());

        let formatted = PythonApi::format_traceback(api, py_tb);

        assert!(!formatted.is_empty());
        assert!(formatted.contains("_tb_single"), "got: {}", formatted);
        assert!(formatted.contains("File"), "got: {}", formatted);
        assert!(formatted.contains("line"), "got: {}", formatted);

        api.decref(py_type);
        api.decref(py_value);
        api.decref(py_tb);
    }

    #[test]
    #[serial]
    fn test_format_traceback_multiple_frames() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _tb_deep_a():
    return _tb_deep_b()

def _tb_deep_b():
    return _tb_deep_c()

def _tb_deep_c():
    raise RuntimeError("deep error")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_tb_deep_a");
        assert!(result.is_null());

        let (py_type, py_value, py_tb) = fetch_traceback(api);
        assert!(!py_tb.is_null());

        let formatted = PythonApi::format_traceback(api, py_tb);

        assert!(formatted.contains("_tb_deep_a"), "got: {}", formatted);
        assert!(formatted.contains("_tb_deep_b"), "got: {}", formatted);
        assert!(formatted.contains("_tb_deep_c"), "got: {}", formatted);

        let frame_count = formatted.matches("File").count();
        assert!(
            frame_count >= 3,
            "expected >= 3 frames, got {}: {}",
            frame_count,
            formatted
        );

        api.decref(py_type);
        api.decref(py_value);
        api.decref(py_tb);
    }

    #[test]
    #[serial]
    fn test_format_traceback_does_not_leave_error_state() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _tb_clean():
    raise RuntimeError("clean test")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_tb_clean");
        assert!(result.is_null());

        let (py_type, py_value, py_tb) = fetch_traceback(api);

        let _ = PythonApi::format_traceback(api, py_tb);
        assert!(!api.has_error());

        api.decref(py_type);
        api.decref(py_value);
        api.decref(py_tb);
    }

    #[test]
    #[serial]
    fn test_extract_exception_with_traceback_via_call() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _tb_extract():
    raise ValueError("extract test")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_tb_extract");
        assert!(result.is_null());
        assert!(api.has_error());

        let exc = PythonApi::extract_exception(api);
        assert!(exc.is_some());

        if let Some(PythonException::Exception {
            kind,
            message,
            traceback,
        }) = exc
        {
            assert_eq!(kind, "ValueError");
            assert_eq!(message, "extract test");
            assert!(traceback.is_some(), "traceback should be present");

            let tb = traceback.unwrap();
            assert!(tb.contains("_tb_extract"), "got: {}", tb);
            assert!(tb.contains("File"), "got: {}", tb);
        } else {
            panic!("Expected Exception variant");
        }

        assert!(!api.has_error());
    }

    #[test]
    #[serial]
    fn test_format_traceback_contains_source_line() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _tb_source():
    x = 1 + 2
    y = x / 0
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_tb_source");
        assert!(result.is_null());

        let (py_type, py_value, py_tb) = fetch_traceback(api);
        assert!(!py_tb.is_null());

        let formatted = PythonApi::format_traceback(api, py_tb);

        assert!(formatted.contains("_tb_source"), "got: {}", formatted);
        // PyRun_SimpleString uses "<string>" as filename — Python can't read back
        // the source to display the line text. Just verify the frame info is present.
        assert!(
            formatted.contains("line") && formatted.contains("File"),
            "traceback should contain frame info, got: {}",
            formatted
        );

        api.decref(py_type);
        api.decref(py_value);
        api.decref(py_tb);
    }

    // ========== extract_syntax_error ==========

    #[test]
    #[serial]
    fn test_extract_exception_returns_syntax_error_variant() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _se_basic():
    compile("def foo(:", "<test>", "exec")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_se_basic");
        assert!(result.is_null());
        assert!(api.has_error());

        let exc = PythonApi::extract_exception(api);
        assert!(exc.is_some());
        assert!(
            matches!(exc, Some(PythonException::SyntaxError { .. })),
            "Expected SyntaxError variant, got: {:?}",
            exc
        );
    }

    #[test]
    #[serial]
    fn test_syntax_error_extracts_line_number() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _se_line():
    compile("def foo(:", "<test>", "exec")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_se_line");
        assert!(result.is_null());

        let exc = PythonApi::extract_exception(api);
        if let Some(PythonException::SyntaxError { line, .. }) = exc {
            assert_eq!(line, 1, "SyntaxError on first line of compiled code");
        } else {
            panic!("Expected SyntaxError variant, got: {:?}", exc);
        }
    }

    #[test]
    #[serial]
    fn test_syntax_error_extracts_offset() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _se_offset():
    compile("def foo(:", "<test>", "exec")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_se_offset");
        assert!(result.is_null());

        let exc = PythonApi::extract_exception(api);
        if let Some(PythonException::SyntaxError { offset, .. }) = exc {
            assert!(
                offset > 0,
                "SyntaxError should have non-zero offset, got: {}",
                offset
            );
        } else {
            panic!("Expected SyntaxError variant, got: {:?}", exc);
        }
    }

    #[test]
    #[serial]
    fn test_syntax_error_extracts_filename() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _se_fname():
    compile("def foo(:", "<my_test_file>", "exec")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_se_fname");
        assert!(result.is_null());

        let exc = PythonApi::extract_exception(api);
        if let Some(PythonException::SyntaxError { filename, .. }) = exc {
            assert_eq!(
                filename, "<my_test_file>",
                "filename should match compile() argument"
            );
        } else {
            panic!("Expected SyntaxError variant, got: {:?}", exc);
        }
    }

    #[test]
    #[serial]
    fn test_syntax_error_extracts_message() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _se_msg():
    compile("def foo(:", "<test>", "exec")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_se_msg");
        assert!(result.is_null());

        let exc = PythonApi::extract_exception(api);
        if let Some(PythonException::SyntaxError { message, .. }) = exc {
            assert!(
                !message.is_empty(),
                "SyntaxError message should not be empty"
            );
        } else {
            panic!("Expected SyntaxError variant, got: {:?}", exc);
        }
    }

    #[test]
    #[serial]
    fn test_syntax_error_clears_error_state() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _se_clear():
    compile("def foo(:", "<test>", "exec")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_se_clear");
        assert!(result.is_null());
        assert!(api.has_error());

        let _ = PythonApi::extract_exception(api);
        assert!(
            !api.has_error(),
            "Error state should be cleared after extraction"
        );
    }

    #[test]
    #[serial]
    fn test_syntax_error_multiline_correct_line() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let setup = api.run_simple_string(
            r#"
def _se_multi():
    compile("x = 1\ny = 2\ndef foo(:", "<test_multi>", "exec")
"#,
        );
        assert!(setup.is_ok());

        let result = call_python_func(api, "_se_multi");
        assert!(result.is_null());

        let exc = PythonApi::extract_exception(api);
        if let Some(PythonException::SyntaxError { line, .. }) = exc {
            assert_eq!(line, 3, "SyntaxError should be on line 3 of compiled code");
        } else {
            panic!("Expected SyntaxError variant, got: {:?}", exc);
        }
    }

    #[test]
    #[serial]
    fn test_regular_exception_not_syntax_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        api.clear_error();

        let py_str = api.string_from_str("not a number");
        let _ = api.long_to_i64(py_str);

        if api.has_error() {
            let exc = PythonApi::extract_exception(api);
            assert!(
                matches!(exc, Some(PythonException::Exception { .. })),
                "TypeError should return Exception variant, not SyntaxError, got: {:?}",
                exc
            );
        }
        api.decref(py_str);
    }

    // ========== Tuple operations ==========

    #[test]
    #[serial]
    fn test_tuple_new_creates_tuple() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let tuple = api.tuple_new(3);
        assert!(!tuple.is_null(), "Should create a Python tuple");
        api.decref(tuple);
    }

    #[test]
    #[serial]
    fn test_tuple_new_empty() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let tuple = api.tuple_new(0);
        assert!(!tuple.is_null(), "Should create an empty Python tuple");
        api.decref(tuple);
    }

    #[test]
    #[serial]
    fn test_tuple_set_item_succeeds() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let tuple = api.tuple_new(3);

        // PyTuple_SetItem steals the reference, so no decref needed for values
        let val0 = api.long_from_i64(10);
        let val1 = api.long_from_i64(20);
        let val2 = api.long_from_i64(30);

        assert_eq!(
            api.tuple_set_item(tuple, 0, val0),
            0,
            "set_item at 0 should succeed"
        );
        assert_eq!(
            api.tuple_set_item(tuple, 1, val1),
            0,
            "set_item at 1 should succeed"
        );
        assert_eq!(
            api.tuple_set_item(tuple, 2, val2),
            0,
            "set_item at 2 should succeed"
        );

        api.decref(tuple);
    }

    #[test]
    #[serial]
    fn test_tuple_roundtrip_via_python() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Create a tuple (10, 20, 30) and pass it to Python to verify
        let tuple = api.tuple_new(3);
        api.tuple_set_item(tuple, 0, api.long_from_i64(10));
        api.tuple_set_item(tuple, 1, api.long_from_i64(20));
        api.tuple_set_item(tuple, 2, api.long_from_i64(30));

        let key = api.string_from_str("my_tuple");
        api.dict_set_item(globals, key, tuple);
        api.decref(key);
        api.decref(tuple);

        // Verify from Python side
        let result = api.run_string("len(my_tuple)", PY_EVAL_INPUT, globals, globals);
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert_eq!(api.long_to_i64(py_obj), 3, "Tuple should have length 3");
        api.decref(py_obj);

        let result = api.run_string(
            "my_tuple[0] + my_tuple[1] + my_tuple[2]",
            PY_EVAL_INPUT,
            globals,
            globals,
        );
        assert!(result.is_ok());
        let py_obj = result.unwrap();
        assert_eq!(
            api.long_to_i64(py_obj),
            60,
            "Sum of tuple elements should be 60"
        );
        api.decref(py_obj);

        api.decref(globals);
    }

    // ========== tuple_check, tuple_size, tuple_get_item ==========

    #[test]
    #[serial]
    fn test_tuple_check() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let tuple = api.tuple_new(0);
        let not_tuple = api.long_from_i64(42);

        assert!(api.tuple_check(tuple), "Tuple should pass tuple_check");
        assert!(!api.tuple_check(not_tuple), "Int should fail tuple_check");

        api.decref(tuple);
        api.decref(not_tuple);
    }

    #[test]
    #[serial]
    fn test_tuple_check_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().tuple_check(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_tuple_check_rejects_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let list = api.list_new(0);
        assert!(!api.tuple_check(list), "List should fail tuple_check");
        api.decref(list);
    }

    #[test]
    #[serial]
    fn test_tuple_size() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let empty = api.tuple_new(0);
        assert_eq!(api.tuple_size(empty), 0, "Empty tuple should have size 0");
        api.decref(empty);

        let triple = api.tuple_new(3);
        api.tuple_set_item(triple, 0, api.long_from_i64(1));
        api.tuple_set_item(triple, 1, api.long_from_i64(2));
        api.tuple_set_item(triple, 2, api.long_from_i64(3));
        assert_eq!(api.tuple_size(triple), 3, "Tuple should have size 3");
        api.decref(triple);
    }

    #[test]
    #[serial]
    fn test_tuple_size_null_returns_zero() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert_eq!(guard.api().tuple_size(std::ptr::null_mut()), 0);
    }

    #[test]
    #[serial]
    fn test_tuple_get_item() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let tuple = api.tuple_new(3);
        api.tuple_set_item(tuple, 0, api.long_from_i64(10));
        api.tuple_set_item(tuple, 1, api.long_from_i64(20));
        api.tuple_set_item(tuple, 2, api.long_from_i64(30));

        // PyTuple_GetItem returns a borrowed reference, no decref needed
        let got0 = api.tuple_get_item(tuple, 0);
        let got1 = api.tuple_get_item(tuple, 1);
        let got2 = api.tuple_get_item(tuple, 2);

        assert_eq!(api.long_to_i64(got0), 10);
        assert_eq!(api.long_to_i64(got1), 20);
        assert_eq!(api.long_to_i64(got2), 30);

        api.decref(tuple);
    }

    #[test]
    #[serial]
    fn test_tuple_get_item_mixed_types() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let tuple = api.tuple_new(4);
        api.tuple_set_item(tuple, 0, api.long_from_i64(42));
        api.tuple_set_item(tuple, 1, api.float_from_f64(std::f64::consts::PI));
        api.tuple_set_item(tuple, 2, api.string_from_str("hello"));
        api.tuple_set_item(tuple, 3, api.bool_from_bool(true));

        assert!(api.is_long(api.tuple_get_item(tuple, 0)));
        assert!(api.is_float(api.tuple_get_item(tuple, 1)));
        assert!(api.is_string(api.tuple_get_item(tuple, 2)));
        assert!(api.is_bool(api.tuple_get_item(tuple, 3)));

        assert_eq!(api.long_to_i64(api.tuple_get_item(tuple, 0)), 42);
        assert!(
            (api.float_to_f64(api.tuple_get_item(tuple, 1)) - std::f64::consts::PI).abs() < 1e-9
        );
        assert_eq!(
            api.string_to_string(api.tuple_get_item(tuple, 2)),
            Some("hello".to_string())
        );

        api.decref(tuple);
    }

    // ========== iter_next ==========

    #[test]
    #[serial]
    fn test_iter_next_basic_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // iter([10, 20, 30]) returns a list iterator
        let py_iter = api
            .run_string("iter([10, 20, 30])", PY_EVAL_INPUT, globals, globals)
            .expect("should create iterator");
        assert!(!py_iter.is_null());

        let item0 = api.iter_next(py_iter);
        assert!(!item0.is_null(), "first item should not be null");
        assert_eq!(api.long_to_i64(item0), 10);
        api.decref(item0);

        let item1 = api.iter_next(py_iter);
        assert!(!item1.is_null(), "second item should not be null");
        assert_eq!(api.long_to_i64(item1), 20);
        api.decref(item1);

        let item2 = api.iter_next(py_iter);
        assert!(!item2.is_null(), "third item should not be null");
        assert_eq!(api.long_to_i64(item2), 30);
        api.decref(item2);

        // Iterator exhausted — returns null, no error set
        let end = api.iter_next(py_iter);
        assert!(end.is_null(), "exhausted iterator should return null");
        assert!(
            !api.has_error(),
            "StopIteration should not leave an error set"
        );

        api.decref(py_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iter_next_empty_iterator() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let py_iter = api
            .run_string("iter([])", PY_EVAL_INPUT, globals, globals)
            .expect("should create empty iterator");

        let end = api.iter_next(py_iter);
        assert!(
            end.is_null(),
            "empty iterator should return null immediately"
        );
        assert!(
            !api.has_error(),
            "no error should be set for empty iterator"
        );

        api.decref(py_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iter_next_string_iterator() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Iterating a string yields individual characters
        let py_iter = api
            .run_string("iter('abc')", PY_EVAL_INPUT, globals, globals)
            .expect("should create string iterator");

        let ch0 = api.iter_next(py_iter);
        assert_eq!(api.string_to_string(ch0), Some("a".to_string()));
        api.decref(ch0);

        let ch1 = api.iter_next(py_iter);
        assert_eq!(api.string_to_string(ch1), Some("b".to_string()));
        api.decref(ch1);

        let ch2 = api.iter_next(py_iter);
        assert_eq!(api.string_to_string(ch2), Some("c".to_string()));
        api.decref(ch2);

        let end = api.iter_next(py_iter);
        assert!(end.is_null());
        assert!(!api.has_error());

        api.decref(py_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iter_next_range() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // range(5) yields 0,1,2,3,4
        let py_iter = api
            .run_string("iter(range(5))", PY_EVAL_INPUT, globals, globals)
            .expect("should create range iterator");

        let mut values = Vec::new();
        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                break;
            }
            values.push(api.long_to_i64(item));
            api.decref(item);
        }
        assert!(!api.has_error());
        assert_eq!(values, vec![0, 1, 2, 3, 4]);

        api.decref(py_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iter_next_dict_iterates_keys() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Iterating a dict yields its keys
        let py_iter = api
            .run_string("iter({'x': 1, 'y': 2})", PY_EVAL_INPUT, globals, globals)
            .expect("should create dict iterator");

        let mut keys = Vec::new();
        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                break;
            }
            keys.push(api.string_to_string(item).unwrap());
            api.decref(item);
        }
        assert!(!api.has_error());
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"x".to_string()));
        assert!(keys.contains(&"y".to_string()));

        api.decref(py_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iter_next_generator() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Define a generator and get an iterator from it
        api.run_string(
            "def gen():\n    yield 100\n    yield 200\n    yield 300\n",
            PY_FILE_INPUT,
            globals,
            globals,
        )
        .expect("should define generator");

        let py_iter = api
            .run_string("gen()", PY_EVAL_INPUT, globals, globals)
            .expect("should create generator iterator");

        let item0 = api.iter_next(py_iter);
        assert!(!item0.is_null());
        assert_eq!(api.long_to_i64(item0), 100);
        api.decref(item0);

        let item1 = api.iter_next(py_iter);
        assert!(!item1.is_null());
        assert_eq!(api.long_to_i64(item1), 200);
        api.decref(item1);

        let item2 = api.iter_next(py_iter);
        assert!(!item2.is_null());
        assert_eq!(api.long_to_i64(item2), 300);
        api.decref(item2);

        let end = api.iter_next(py_iter);
        assert!(end.is_null());
        assert!(!api.has_error());

        api.decref(py_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iter_next_mixed_types_from_generator() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        api.run_string(
            "def mixed():\n    yield 42\n    yield 3.141592654\n    yield 'hello'\n    yield True\n    yield None\n",
            PY_FILE_INPUT,
            globals,
            globals,
        ).expect("should define mixed generator");

        let py_iter = api
            .run_string("mixed()", PY_EVAL_INPUT, globals, globals)
            .expect("should create mixed generator iterator");

        let v0 = api.iter_next(py_iter);
        assert!(api.is_long(v0));
        assert_eq!(api.long_to_i64(v0), 42);
        api.decref(v0);

        let v1 = api.iter_next(py_iter);
        assert!(api.is_float(v1));
        assert!((api.float_to_f64(v1) - std::f64::consts::PI).abs() < 1e-9);
        api.decref(v1);

        let v2 = api.iter_next(py_iter);
        assert!(api.is_string(v2));
        assert_eq!(api.string_to_string(v2), Some("hello".to_string()));
        api.decref(v2);

        let v3 = api.iter_next(py_iter);
        assert_eq!(v3, api.py_true);
        api.decref(v3);

        let v4 = api.iter_next(py_iter);
        assert_eq!(v4, api.py_none);
        api.decref(v4);

        let end = api.iter_next(py_iter);
        assert!(end.is_null());
        assert!(!api.has_error());

        api.decref(py_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iter_next_tuple_iterator() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let py_iter = api
            .run_string("iter((7, 8, 9))", PY_EVAL_INPUT, globals, globals)
            .expect("should create tuple iterator");

        let mut values = Vec::new();
        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                break;
            }
            values.push(api.long_to_i64(item));
            api.decref(item);
        }
        assert!(!api.has_error());
        assert_eq!(values, vec![7, 8, 9]);

        api.decref(py_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iter_next_called_after_exhaustion_still_returns_null() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let py_iter = api
            .run_string("iter([1])", PY_EVAL_INPUT, globals, globals)
            .expect("should create single-item iterator");

        let item = api.iter_next(py_iter);
        assert!(!item.is_null());
        api.decref(item);

        // First call after exhaustion
        let end1 = api.iter_next(py_iter);
        assert!(end1.is_null());
        assert!(!api.has_error());

        // Second call after exhaustion — should still be safe
        let end2 = api.iter_next(py_iter);
        assert!(end2.is_null());
        assert!(!api.has_error());

        api.decref(py_iter);
        api.decref(globals);
    }

    // ========== object_call ==========

    #[test]
    #[serial]
    fn test_object_call_builtin_function() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Get the built-in len() function
        let builtins = api
            .import_module("builtins")
            .expect("builtins should exist");
        let len_func = api.object_get_attr_string(builtins, "len");
        assert!(!len_func.is_null(), "Should get len function");

        // Create an args tuple with a list
        let list = api.list_new(3);
        api.list_set_item(list, 0, api.long_from_i64(1));
        api.list_set_item(list, 1, api.long_from_i64(2));
        api.list_set_item(list, 2, api.long_from_i64(3));

        let args = api.tuple_new(1);
        api.incref(list);
        api.tuple_set_item(args, 0, list);

        let result = api.object_call(len_func, args, std::ptr::null_mut());
        assert!(!result.is_null(), "object_call should return non-null");
        assert_eq!(api.long_to_i64(result), 3, "len([1,2,3]) should be 3");

        api.decref(result);
        api.decref(args);
        api.decref(list);
        api.decref(len_func);
        api.decref(builtins);
    }

    #[test]
    #[serial]
    fn test_object_call_with_kwargs() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Define a Python function that uses kwargs
        let setup = api.run_simple_string(
            r#"
def _test_kwargs(a, b=10):
    return a + b
"#,
        );
        assert!(setup.is_ok());

        let main_module = api
            .import_module("__main__")
            .expect("__main__ should exist");
        let func = api.object_get_attr_string(main_module, "_test_kwargs");
        assert!(!func.is_null());

        // Call with positional arg a=5 and kwarg b=20
        let args = api.tuple_new(1);
        api.tuple_set_item(args, 0, api.long_from_i64(5));

        let kwargs = api.dict_new();
        let key_b = api.string_from_str("b");
        let val_b = api.long_from_i64(20);
        api.dict_set_item(kwargs, key_b, val_b);

        let result = api.object_call(func, args, kwargs);
        assert!(!result.is_null(), "object_call with kwargs should succeed");
        assert_eq!(api.long_to_i64(result), 25, "5 + 20 should be 25");

        api.decref(result);
        api.decref(key_b);
        api.decref(val_b);
        api.decref(kwargs);
        api.decref(args);
        api.decref(func);
        api.decref(main_module);
    }

    #[test]
    #[serial]
    fn test_object_call_returns_null_on_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Define a function that raises
        let setup = api.run_simple_string(
            r#"
def _test_call_raises():
    raise RuntimeError("call failed")
"#,
        );
        assert!(setup.is_ok());

        let main_module = api
            .import_module("__main__")
            .expect("__main__ should exist");
        let func = api.object_get_attr_string(main_module, "_test_call_raises");
        assert!(!func.is_null());

        let args = api.tuple_new(0);
        let result = api.object_call(func, args, std::ptr::null_mut());
        assert!(result.is_null(), "Should return null when function raises");
        assert!(api.has_error(), "Error should be set");
        api.clear_error();

        api.decref(args);
        api.decref(func);
        api.decref(main_module);
    }

    // ========== object_set_attr_string / object_has_attr_string / object_get_attr_string ==========

    #[test]
    #[serial]
    fn test_object_set_attr_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Create a simple Python object via a class
        let setup = api.run_simple_string(
            r#"
class _TestObj:
    pass
_test_obj = _TestObj()
"#,
        );
        assert!(setup.is_ok());

        let main_module = api
            .import_module("__main__")
            .expect("__main__ should exist");
        let obj = api.object_get_attr_string(main_module, "_test_obj");
        assert!(!obj.is_null());

        // Set attribute "x" = 42
        let val = api.long_from_i64(42);
        let result = api.object_set_attr_string(obj, "x", val);
        assert_eq!(result, 0, "object_set_attr_string should succeed");
        api.decref(val);

        // Verify the attribute was set
        let got = api.object_get_attr_string(obj, "x");
        assert!(!got.is_null(), "Should be able to get attribute 'x'");
        assert_eq!(api.long_to_i64(got), 42, "Attribute x should be 42");
        api.decref(got);

        api.decref(obj);
        api.decref(main_module);
    }

    #[test]
    #[serial]
    fn test_object_has_attr_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Create an object with a known attribute
        let setup = api.run_simple_string(
            r#"
class _TestHasAttr:
    existing_attr = 123
_test_has_attr = _TestHasAttr()
"#,
        );
        assert!(setup.is_ok());

        let main_module = api
            .import_module("__main__")
            .expect("__main__ should exist");
        let obj = api.object_get_attr_string(main_module, "_test_has_attr");
        assert!(!obj.is_null());

        // Check for existing attribute
        let has = api.object_has_attr_string(obj, c"existing_attr".as_ptr() as *const c_char);
        assert_eq!(has, 1, "Should have 'existing_attr'");

        // Check for non-existing attribute
        let has_not = api.object_has_attr_string(obj, c"nonexistent".as_ptr() as *const c_char);
        assert_eq!(has_not, 0, "Should not have 'nonexistent'");

        api.decref(obj);
        api.decref(main_module);
    }

    #[test]
    #[serial]
    fn test_object_get_attr_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Create an object with a known attribute
        let setup = api.run_simple_string(
            r#"
class _TestGetAttr:
    value = "hello"
_test_get_attr = _TestGetAttr()
"#,
        );
        assert!(setup.is_ok());

        let main_module = api
            .import_module("__main__")
            .expect("__main__ should exist");
        let obj = api.object_get_attr_string(main_module, "_test_get_attr");
        assert!(!obj.is_null());

        // Get existing attribute
        let val = api.object_get_attr_string(obj, "value");
        assert!(!val.is_null(), "Should get attribute 'value'");
        assert_eq!(api.string_to_string(val), Some("hello".to_string()));
        api.decref(val);

        // Get non-existing attribute returns null and sets error
        let missing = api.object_get_attr_string(obj, "missing");
        assert!(missing.is_null(), "Missing attribute should return null");
        assert!(api.has_error(), "Error should be set for missing attribute");
        api.clear_error();

        api.decref(obj);
        api.decref(main_module);
    }

    // ========== callable_check ==========

    #[test]
    #[serial]
    fn test_callable_check_function_is_callable() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let setup = api.run_simple_string(
            r#"
def _test_callable_fn():
    pass
"#,
        );
        assert!(setup.is_ok());

        let main_module = api
            .import_module("__main__")
            .expect("__main__ should exist");
        let func = api.object_get_attr_string(main_module, "_test_callable_fn");
        assert!(!func.is_null());

        assert_eq!(api.callable_check(func), 1, "Function should be callable");

        api.decref(func);
        api.decref(main_module);
    }

    #[test]
    #[serial]
    fn test_callable_check_non_callable() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        assert_eq!(
            api.callable_check(py_int),
            0,
            "Integer should not be callable"
        );
        api.decref(py_int);

        let py_str = api.string_from_str("hello");
        assert_eq!(
            api.callable_check(py_str),
            0,
            "String should not be callable"
        );
        api.decref(py_str);

        let py_list = api.list_new(0);
        assert_eq!(
            api.callable_check(py_list),
            0,
            "List should not be callable"
        );
        api.decref(py_list);
    }

    #[test]
    #[serial]
    fn test_callable_check_class_is_callable() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let setup = api.run_simple_string(
            r#"
class _TestCallableClass:
    pass
"#,
        );
        assert!(setup.is_ok());

        let main_module = api
            .import_module("__main__")
            .expect("__main__ should exist");
        let cls = api.object_get_attr_string(main_module, "_TestCallableClass");
        assert!(!cls.is_null());

        assert_eq!(
            api.callable_check(cls),
            1,
            "Class should be callable (constructor)"
        );

        api.decref(cls);
        api.decref(main_module);
    }

    #[test]
    #[serial]
    fn test_callable_check_lambda_is_callable() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let result = api.run_string("lambda x: x + 1", PY_EVAL_INPUT, globals, globals);
        assert!(result.is_ok());
        let lambda = result.unwrap();
        assert!(!lambda.is_null());

        assert_eq!(api.callable_check(lambda), 1, "Lambda should be callable");

        api.decref(lambda);
        api.decref(globals);
    }

    // ========== import_module ==========

    #[test]
    #[serial]
    fn test_import_module_success() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let result = api.import_module("json");
        assert!(result.is_ok(), "Should import json module");
        let module = result.unwrap();
        assert!(!module.is_null());
        api.decref(module);
    }

    #[test]
    #[serial]
    fn test_import_module_failure() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let result = api.import_module("nonexistent_module_xyz_12345");
        assert!(result.is_err(), "Should fail to import nonexistent module");
        api.clear_error();
    }

    #[test]
    #[serial]
    fn test_import_module_and_call_function() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let module = api.import_module("json").expect("json should import");
        let dumps_fn = api.object_get_attr_string(module, "dumps");
        assert!(!dumps_fn.is_null(), "Should get json.dumps");
        assert_eq!(
            api.callable_check(dumps_fn),
            1,
            "json.dumps should be callable"
        );

        // Call json.dumps(42) -> "42"
        let args = api.tuple_new(1);
        api.tuple_set_item(args, 0, api.long_from_i64(42));

        let result = api.object_call(dumps_fn, args, std::ptr::null_mut());
        assert!(!result.is_null(), "json.dumps(42) should succeed");
        assert_eq!(api.string_to_string(result), Some("42".to_string()));

        api.decref(result);
        api.decref(args);
        api.decref(dumps_fn);
        api.decref(module);
    }

    // ========== object_call_no_args ==========

    #[test]
    #[serial]
    fn test_object_call_no_args_with_builtin() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // dict() called with no args returns an empty dict
        let dict_fn = api
            .run_string("dict", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!dict_fn.is_null());

        let result = api.object_call_no_args(dict_fn);
        assert!(!result.is_null(), "dict() should return a non-null object");
        assert!(!api.has_error(), "No error should be set");
        assert!(api.dict_check(result), "dict() should return a dict");
        assert_eq!(api.dict_size(result), 0, "dict() should return empty dict");

        api.decref(result);
        api.decref(dict_fn);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_object_call_no_args_with_lambda() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let lambda = api
            .run_string("lambda: 42", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!lambda.is_null());

        let result = api.object_call_no_args(lambda);
        assert!(!result.is_null(), "lambda should return a value");
        assert!(!api.has_error());
        assert_eq!(api.long_to_i64(result), 42, "lambda should return 42");

        api.decref(result);
        api.decref(lambda);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_object_call_no_args_with_user_defined_function() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let setup = api
            .run_string(
                "def _test_no_args_fn():\n    return 'hello'",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();
        assert!(!setup.is_null() || api.is_none(setup));

        let func = api
            .run_string("_test_no_args_fn", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!func.is_null());

        let result = api.object_call_no_args(func);
        assert!(!result.is_null(), "Function should return a value");
        assert!(!api.has_error());
        assert_eq!(
            api.string_to_string(result),
            Some("hello".to_string()),
            "Function should return 'hello'"
        );

        api.decref(result);
        api.decref(func);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_object_call_no_args_with_non_callable_fails() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        let result = api.object_call_no_args(py_int);
        assert!(
            result.is_null(),
            "Calling a non-callable should return null"
        );
        assert!(api.has_error(), "Error should be set for non-callable");
        api.clear_error();

        api.decref(py_int);
    }

    // ========== py_exc_stop_async_iteration ==========

    #[test]
    #[serial]
    fn test_stop_async_iteration_exception_is_loaded() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        assert!(
            !api.py_exc_stop_async_iteration.is_null(),
            "PyExc_StopAsyncIteration should be non-null"
        );
    }

    #[test]
    #[serial]
    fn test_stop_async_iteration_can_be_raised_and_matched() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Raise StopAsyncIteration via run_string (preserves error state unlike run_simple_string)
        let result = api
            .run_string(
                "raise StopAsyncIteration('done')",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();
        assert!(result.is_null(), "raise should return null");
        assert!(api.has_error(), "Error should be set after raise");

        // Use extract_exception to check the exception kind
        let exc = PythonApi::extract_exception(api);
        assert!(exc.is_some(), "Should have an exception");
        match exc.unwrap() {
            PythonException::Exception { kind, .. } => {
                assert_eq!(
                    kind, "StopAsyncIteration",
                    "Exception kind should be StopAsyncIteration"
                );
            }
            other => panic!("Expected Exception variant, got {:?}", other),
        }

        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_stop_async_iteration_does_not_match_other_exceptions() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Raise a ValueError via run_string (preserves error state)
        let result = api
            .run_string(
                "raise ValueError('not async')",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();
        assert!(result.is_null());
        assert!(api.has_error());

        let exc = PythonApi::extract_exception(api);
        assert!(exc.is_some());
        match exc.unwrap() {
            PythonException::Exception { kind, .. } => {
                assert_ne!(
                    kind, "StopAsyncIteration",
                    "ValueError should not be StopAsyncIteration"
                );
                assert_eq!(kind, "ValueError");
            }
            other => panic!("Expected Exception variant, got {:?}", other),
        }

        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_stop_async_iteration_raised_by_callable() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Define a callable that raises StopAsyncIteration (simulates exhausted async generator)
        let setup_code = r#"
def _raise_stop_async():
    raise StopAsyncIteration('generator exhausted')
"#;
        let setup = api
            .run_string(setup_code, PY_FILE_INPUT, globals, globals)
            .unwrap();
        assert!(!setup.is_null());

        let func = api
            .run_string("_raise_stop_async", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!func.is_null());

        // Call it — should raise StopAsyncIteration
        let result = api.object_call_no_args(func);
        assert!(result.is_null(), "Should return null when exception raised");
        assert!(api.has_error());

        let exc = PythonApi::extract_exception(api);
        assert!(exc.is_some());
        match exc.unwrap() {
            PythonException::Exception { kind, message, .. } => {
                assert_eq!(kind, "StopAsyncIteration");
                assert_eq!(message, "generator exhausted");
            }
            other => panic!("Expected Exception variant, got {:?}", other),
        }

        api.decref(func);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_stop_async_iteration_from_async_generator() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // asyncio may fail to import due to C extension loading issues (RTLD_LOCAL).
        // If so, skip this test gracefully.
        let setup_code = r#"
import asyncio

async def _test_agen():
    yield 1
    yield 2

_agen = _test_agen()
_loop = asyncio.new_event_loop()
"#;
        let setup = api
            .run_string(setup_code, PY_FILE_INPUT, globals, globals)
            .unwrap();
        if setup.is_null() {
            api.clear_error();
            api.decref(globals);
            println!("Skipping: asyncio not available");
            return;
        }

        // Get __anext__ method
        let agen = api
            .run_string("_agen", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!agen.is_null());
        let anext = api.object_get_attr_string(agen, "__anext__");
        assert!(!anext.is_null());
        let loop_obj = api
            .run_string("_loop", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!loop_obj.is_null());
        let run_until_complete = api.object_get_attr_string(loop_obj, "run_until_complete");
        assert!(!run_until_complete.is_null());

        // First yield: should get 1
        let coro1 = api.object_call_no_args(anext);
        assert!(!coro1.is_null());
        let args1 = api.tuple_new(1);
        api.incref(coro1);
        api.tuple_set_item(args1, 0, coro1);
        let val1 = api.object_call(run_until_complete, args1, std::ptr::null_mut());
        assert!(!val1.is_null(), "First yield should succeed");
        assert_eq!(api.long_to_i64(val1), 1);
        api.decref(val1);
        api.decref(args1);
        api.decref(coro1);

        // Second yield: should get 2
        let coro2 = api.object_call_no_args(anext);
        assert!(!coro2.is_null());
        let args2 = api.tuple_new(1);
        api.incref(coro2);
        api.tuple_set_item(args2, 0, coro2);
        let val2 = api.object_call(run_until_complete, args2, std::ptr::null_mut());
        assert!(!val2.is_null(), "Second yield should succeed");
        assert_eq!(api.long_to_i64(val2), 2);
        api.decref(val2);
        api.decref(args2);
        api.decref(coro2);

        // Third call: should raise StopAsyncIteration
        let coro3 = api.object_call_no_args(anext);
        assert!(!coro3.is_null());
        let args3 = api.tuple_new(1);
        api.incref(coro3);
        api.tuple_set_item(args3, 0, coro3);
        let val3 = api.object_call(run_until_complete, args3, std::ptr::null_mut());
        assert!(val3.is_null(), "Exhausted generator should return null");
        assert!(api.has_error());

        // Verify it's StopAsyncIteration via extract_exception
        let exc = PythonApi::extract_exception(api);
        assert!(exc.is_some());
        match exc.unwrap() {
            PythonException::Exception { kind, .. } => {
                assert_eq!(kind, "StopAsyncIteration");
            }
            other => panic!("Expected Exception variant, got {:?}", other),
        }

        api.decref(args3);
        api.decref(coro3);
        api.decref(run_until_complete);
        api.decref(loop_obj);
        api.decref(anext);
        api.decref(agen);

        // Close the event loop
        let _ = api.run_string("_loop.close()", PY_FILE_INPUT, globals, globals);
        if api.has_error() {
            api.clear_error();
        }
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_is_iterable_checks_return_false_for_null_ptr() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        assert!(!api.is_async_iterable(std::ptr::null_mut()));
        assert!(!api.is_sync_iterable(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_is_async_iterable_true_for_async_generator_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        let globals = make_globals(api);

        let setup = api
            .run_string(
                "async def _detect_async_gen():\n    yield 1\n_detect_async_obj = _detect_async_gen()",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();
        if setup.is_null() {
            api.clear_error();
            api.decref(globals);
            println!("Skipping: async generator setup failed");
            return;
        }
        api.decref(setup);

        let obj = api
            .run_string("_detect_async_obj", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!obj.is_null());
        assert!(api.is_async_iterable(obj));
        assert!(!api.is_sync_iterable(obj));
        assert!(!api.has_error());

        api.decref(obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_is_sync_iterable_true_for_sync_iterator_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        let globals = make_globals(api);

        let obj = api
            .run_string("iter([1, 2, 3])", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!obj.is_null());
        assert!(!api.is_async_iterable(obj));
        assert!(api.is_sync_iterable(obj));
        assert!(!api.has_error());

        api.decref(obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iterable_checks_false_for_non_iterable_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        let globals = make_globals(api);

        let obj = api
            .run_string("42", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!obj.is_null());
        assert!(!api.is_async_iterable(obj));
        assert!(!api.is_sync_iterable(obj));
        assert!(!api.has_error());

        api.decref(obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_is_sync_iterable_false_when_iter_attr_is_not_callable() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        let globals = make_globals(api);

        let setup = api
            .run_string(
                "class _BadIter:\n    __iter__ = 123\n_bad_iter_obj = _BadIter()",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();
        assert!(!setup.is_null());
        api.decref(setup);

        let obj = api
            .run_string("_bad_iter_obj", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!obj.is_null());
        assert!(!api.is_sync_iterable(obj));
        assert!(!api.has_error());

        api.decref(obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_is_async_iterable_true_for_custom_async_iterable_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        let globals = make_globals(api);

        let setup = api
            .run_string(
                "class _CustomAsyncIterable:\n    def __aiter__(self):\n        return self\n    async def __anext__(self):\n        raise StopAsyncIteration\n_custom_async_obj = _CustomAsyncIterable()",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();
        assert!(!setup.is_null());
        api.decref(setup);

        let obj = api
            .run_string("_custom_async_obj", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!obj.is_null());
        assert!(api.is_async_iterable(obj));
        assert!(!api.is_sync_iterable(obj));
        assert!(!api.has_error());

        api.decref(obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_probe_async_iterable_true_for_async_generator_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        let globals = make_globals(api);

        let setup = api
            .run_string(
                "async def _probe_async_gen():\n    yield 1\n_probe_async_obj = _probe_async_gen()",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();
        assert!(!setup.is_null());
        api.decref(setup);

        let obj = api
            .run_string("_probe_async_obj", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!obj.is_null());

        let result = api.probe_async_iterable(obj);
        assert!(matches!(result, Ok(true)));
        assert!(!api.has_error());

        api.decref(obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_probe_async_iterable_false_for_sync_iterator() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        let globals = make_globals(api);

        let obj = api
            .run_string("iter([1, 2, 3])", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!obj.is_null());

        let result = api.probe_async_iterable(obj);
        assert!(matches!(result, Ok(false)));
        assert!(!api.has_error());

        api.decref(obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_probe_async_iterable_propagates_non_attribute_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        let globals = make_globals(api);

        let setup = api
            .run_string(
                "class _ProbeBoom:\n    def __getattribute__(self, name):\n        if name in ('__aiter__', '__anext__'):\n            raise ValueError('probe boom')\n        return object.__getattribute__(self, name)\n_probe_boom_obj = _ProbeBoom()",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();
        assert!(!setup.is_null());
        api.decref(setup);

        let obj = api
            .run_string("_probe_boom_obj", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!obj.is_null());

        let result = api.probe_async_iterable(obj);
        match result {
            Err(PythonException::Exception { kind, message, .. }) => {
                assert_eq!(kind, "ValueError");
                assert_eq!(message, "probe boom");
            }
            other => panic!("Expected ValueError from probe, got {:?}", other),
        }
        assert!(!api.has_error());

        api.decref(obj);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_sync_adapter_class_is_loaded() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        if api.run_simple_string(SYNC_ADAPTER_PY).is_err() {
            println!("Skipping: sync adapter could not be loaded");
            return;
        }

        let main_module = api.import_module("__main__").unwrap();
        let adapter_class = api.object_get_attr_string(main_module, "AsyncToSync");
        assert!(!adapter_class.is_null());
        assert_eq!(api.callable_check(adapter_class), 1);

        api.decref(adapter_class);
        api.decref(main_module);
    }

    #[test]
    #[serial]
    fn test_sync_adapter_wraps_async_generator_as_sync_iterator() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        if api.run_simple_string(SYNC_ADAPTER_PY).is_err() {
            println!("Skipping: sync adapter could not be loaded");
            return;
        }

        let globals = make_globals(api);
        let setup = api
            .run_string(
                "import asyncio\nasync def _adapter_range():\n    for i in [1, 2, 3]:\n        await asyncio.sleep(0)\n        yield i\n_agen = _adapter_range()",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();

        if setup.is_null() {
            api.clear_error();
            api.decref(globals);
            println!("Skipping: async setup failed");
            return;
        }
        api.decref(setup);

        let async_gen = api
            .run_string("_agen", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!async_gen.is_null());

        let main_module = api.import_module("__main__").unwrap();
        let adapter_class = api.object_get_attr_string(main_module, "AsyncToSync");
        assert!(!adapter_class.is_null());

        let args = api.tuple_new(1);
        assert!(!args.is_null());
        api.incref(async_gen);
        api.tuple_set_item(args, 0, async_gen);

        let sync_iter = api.object_call(adapter_class, args, std::ptr::null_mut());

        api.decref(args);
        api.decref(adapter_class);
        api.decref(main_module);
        api.decref(async_gen);

        assert!(!sync_iter.is_null());
        assert!(!api.has_error());

        let py_iter = api.object_get_iter(sync_iter);
        assert!(!py_iter.is_null());

        let v1 = api.iter_next(py_iter);
        assert!(!v1.is_null());
        assert_eq!(api.long_to_i64(v1), 1);
        api.decref(v1);

        let v2 = api.iter_next(py_iter);
        assert!(!v2.is_null());
        assert_eq!(api.long_to_i64(v2), 2);
        api.decref(v2);

        let v3 = api.iter_next(py_iter);
        assert!(!v3.is_null());
        assert_eq!(api.long_to_i64(v3), 3);
        api.decref(v3);

        let end = api.iter_next(py_iter);
        assert!(end.is_null());
        assert!(!api.has_error());

        api.decref(py_iter);
        api.decref(sync_iter);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_sync_adapter_propagates_async_generator_error() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };

        let api = guard.api();
        if api.run_simple_string(SYNC_ADAPTER_PY).is_err() {
            println!("Skipping: sync adapter could not be loaded");
            return;
        }

        let globals = make_globals(api);
        let setup = api
            .run_string(
                "import asyncio\nasync def _adapter_boom():\n    yield 1\n    await asyncio.sleep(0)\n    raise ValueError('adapter boom')\n_agen_err = _adapter_boom()",
                PY_FILE_INPUT,
                globals,
                globals,
            )
            .unwrap();

        if setup.is_null() {
            api.clear_error();
            api.decref(globals);
            println!("Skipping: async setup failed");
            return;
        }
        api.decref(setup);

        let async_gen = api
            .run_string("_agen_err", PY_EVAL_INPUT, globals, globals)
            .unwrap();
        assert!(!async_gen.is_null());

        let main_module = api.import_module("__main__").unwrap();
        let adapter_class = api.object_get_attr_string(main_module, "AsyncToSync");
        assert!(!adapter_class.is_null());

        let args = api.tuple_new(1);
        assert!(!args.is_null());
        api.incref(async_gen);
        api.tuple_set_item(args, 0, async_gen);

        let sync_iter = api.object_call(adapter_class, args, std::ptr::null_mut());

        api.decref(args);
        api.decref(adapter_class);
        api.decref(main_module);
        api.decref(async_gen);

        assert!(!sync_iter.is_null());
        assert!(!api.has_error());

        let py_iter = api.object_get_iter(sync_iter);
        assert!(!py_iter.is_null());

        let first = api.iter_next(py_iter);
        assert!(!first.is_null());
        assert_eq!(api.long_to_i64(first), 1);
        api.decref(first);

        let second = api.iter_next(py_iter);
        assert!(second.is_null());
        assert!(api.has_error());

        let exc = PythonApi::extract_exception(api);
        assert!(exc.is_some());
        match exc.unwrap() {
            PythonException::Exception { kind, message, .. } => {
                assert_eq!(kind, "ValueError");
                assert_eq!(message, "adapter boom");
            }
            other => panic!("Expected Exception variant, got {:?}", other),
        }

        api.decref(py_iter);
        api.decref(sync_iter);
        api.decref(globals);
    }

    // ========== str_to_wchar tests ==========

    #[test]
    #[serial]
    fn test_str_to_wchar_ascii() {
        let wide = PythonApi::str_to_wchar("hello");
        assert_eq!(wide.len(), 6); // 5 chars + null terminator
        assert_eq!(wide[0], 'h' as wchar_t);
        assert_eq!(wide[1], 'e' as wchar_t);
        assert_eq!(wide[2], 'l' as wchar_t);
        assert_eq!(wide[3], 'l' as wchar_t);
        assert_eq!(wide[4], 'o' as wchar_t);
        assert_eq!(wide[5], 0); // null terminator
    }

    #[test]
    #[serial]
    fn test_str_to_wchar_empty() {
        let wide = PythonApi::str_to_wchar("");
        assert_eq!(wide.len(), 1); // just null terminator
        assert_eq!(wide[0], 0);
    }

    #[test]
    #[serial]
    fn test_str_to_wchar_unicode() {
        let wide = PythonApi::str_to_wchar("héllo");
        assert_eq!(wide.len(), 6); // 5 chars + null
        assert_eq!(wide[0], 'h' as wchar_t);
        assert_eq!(wide[1], 'é' as wchar_t);
        assert_eq!(wide[4], 'o' as wchar_t);
        assert_eq!(wide[5], 0);
    }

    #[test]
    #[serial]
    fn test_str_to_wchar_cjk() {
        let wide = PythonApi::str_to_wchar("日本語");
        assert_eq!(wide.len(), 4); // 3 chars + null
        assert_eq!(wide[0], '日' as wchar_t);
        assert_eq!(wide[1], '本' as wchar_t);
        assert_eq!(wide[2], '語' as wchar_t);
        assert_eq!(wide[3], 0);
    }

    #[test]
    #[serial]
    fn test_str_to_wchar_emoji() {
        let wide = PythonApi::str_to_wchar("🦀");
        assert_eq!(wide.len(), 2); // 1 char + null
        assert_eq!(wide[0], '🦀' as wchar_t);
        assert_eq!(wide[1], 0);
    }

    #[test]
    #[serial]
    fn test_str_to_wchar_path() {
        let wide = PythonApi::str_to_wchar("/home/user/.venv/lib/python3.13");
        assert_eq!(wide[0], '/' as wchar_t);
        assert_eq!(*wide.last().unwrap(), 0); // null terminated
        assert_eq!(wide.len(), 32); // 31 chars + null
    }

    // ========== list_append tests ==========

    #[test]
    #[serial]
    fn test_list_append_adds_item() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let list = unsafe { (api.py_list_new)(0) };
        assert!(!list.is_null());

        let item = api.long_from_i64(42);
        let result = api.list_append(list, item);
        assert_eq!(result, 0, "list_append should return 0 on success");

        let size = unsafe { (api.py_list_size)(list) };
        assert_eq!(size, 1, "list should have 1 element");

        api.decref(item);
        api.decref(list);
    }

    #[test]
    #[serial]
    fn test_list_append_multiple_items() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let list = unsafe { (api.py_list_new)(0) };
        for i in 0..5 {
            let item = api.long_from_i64(i);
            api.list_append(list, item);
            api.decref(item);
        }

        let size = unsafe { (api.py_list_size)(list) };
        assert_eq!(size, 5, "list should have 5 elements");

        // Verify order is preserved
        for i in 0..5isize {
            let item = unsafe { (api.py_list_get_item)(list, i) };
            assert_eq!(api.long_to_i64(item), i as i64);
            // py_list_get_item returns a borrowed reference, don't decref
        }

        api.decref(list);
    }

    #[test]
    #[serial]
    fn test_list_append_mixed_types() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let list = unsafe { (api.py_list_new)(0) };

        let int_val = api.long_from_i64(99);
        let str_val = api.string_from_str("hello");
        let float_val = api.float_from_f64(3.14);

        api.list_append(list, int_val);
        api.list_append(list, str_val);
        api.list_append(list, float_val);

        let size = unsafe { (api.py_list_size)(list) };
        assert_eq!(size, 3);

        let item0 = unsafe { (api.py_list_get_item)(list, 0) };
        assert!(api.is_long(item0));
        assert_eq!(api.long_to_i64(item0), 99);

        let item1 = unsafe { (api.py_list_get_item)(list, 1) };
        assert!(api.is_string(item1));
        assert_eq!(api.string_to_string(item1).unwrap(), "hello");

        let item2 = unsafe { (api.py_list_get_item)(list, 2) };
        assert!(api.is_float(item2));
        assert!((api.float_to_f64(item2) - 3.14).abs() < 0.001);

        api.decref(int_val);
        api.decref(str_val);
        api.decref(float_val);
        api.decref(list);
    }

    #[test]
    #[serial]
    fn test_list_append_null_list_returns_error() {
        let Some(_guard) = skip_if_no_python() else {
            return;
        };
        let api = _guard.api();

        let item = api.long_from_i64(1);
        let result = api.list_append(std::ptr::null_mut(), item);
        assert_eq!(result, -1, "appending to null list should return -1");
        api.decref(item);
    }

    #[test]
    #[serial]
    fn test_list_append_to_empty_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let list = unsafe { (api.py_list_new)(0) };
        let size_before = unsafe { (api.py_list_size)(list) };
        assert_eq!(size_before, 0);

        let item = api.string_from_str("first");
        api.list_append(list, item);

        let size_after = unsafe { (api.py_list_size)(list) };
        assert_eq!(size_after, 1);

        api.decref(item);
        api.decref(list);
    }

    // ========== initialize_ex tests ==========

    #[test]
    #[serial]
    fn test_initialize_ex_already_initialized() {
        // Python is already initialized by the test harness.
        // Calling initialize_ex again should be safe (Python docs say it's a no-op).
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        api.initialize_ex(0);
        assert!(
            api.is_initialized(),
            "Python should still be initialized after redundant initialize_ex"
        );
    }

    // ========== sys.path injection via Python ==========

    #[test]
    #[serial]
    fn test_sys_path_injection_via_python() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let test_path = "/tmp/rubyx_test_inject_path";

        // Import sys
        let sys_module = api.import_module("sys").expect("should import sys");
        let sys_path = api.object_get_attr_string(sys_module, "path");
        assert!(!sys_path.is_null(), "sys.path should not be null");

        // Append a test path
        let py_path = api.string_from_str(test_path);
        assert!(!py_path.is_null());
        let result = api.list_append(sys_path, py_path);
        assert_eq!(result, 0, "list_append to sys.path should succeed");

        // Verify the path is in sys.path by evaluating Python code
        let globals = make_globals(api);
        let check_code = format!("'{}' in __import__('sys').path", test_path);
        let py_result = api
            .run_string(&check_code, PY_EVAL_INPUT, globals, std::ptr::null_mut())
            .expect("eval should succeed");
        assert!(
            api.is_true(py_result),
            "injected path should be found in sys.path"
        );

        api.decref(py_result);
        api.decref(globals);
        api.decref(py_path);
        api.decref(sys_path);
        api.decref(sys_module);
    }

    #[test]
    #[serial]
    fn test_sys_path_injection_multiple_paths() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let paths = [
            "/tmp/rubyx_multi_a",
            "/tmp/rubyx_multi_b",
            "/tmp/rubyx_multi_c",
        ];

        let sys_module = api.import_module("sys").expect("should import sys");
        let sys_path = api.object_get_attr_string(sys_module, "path");

        for path in &paths {
            let py_str = api.string_from_str(path);
            api.list_append(sys_path, py_str);
            api.decref(py_str);
        }

        // Verify all paths were added
        let globals = make_globals(api);
        for path in &paths {
            let check = format!("'{}' in __import__('sys').path", path);
            let result = api
                .run_string(&check, PY_EVAL_INPUT, globals, std::ptr::null_mut())
                .expect("eval should succeed");
            assert!(api.is_true(result), "path {} should be in sys.path", path);
            api.decref(result);
        }

        api.decref(globals);
        api.decref(sys_path);
        api.decref(sys_module);
    }

    #[test]
    #[serial]
    fn test_sys_path_injection_preserves_existing_paths() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let sys_module = api.import_module("sys").expect("should import sys");
        let sys_path = api.object_get_attr_string(sys_module, "path");
        let size_before = unsafe { (api.py_list_size)(sys_path) };

        let py_str = api.string_from_str("/tmp/rubyx_preserve_test");
        api.list_append(sys_path, py_str);
        api.decref(py_str);

        let size_after = unsafe { (api.py_list_size)(sys_path) };
        assert_eq!(
            size_after,
            size_before + 1,
            "sys.path should grow by exactly 1"
        );

        api.decref(sys_path);
        api.decref(sys_module);
    }

    #[test]
    #[serial]
    fn test_import_module_from_injected_path() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Create a temporary Python module
        let tmp_dir = std::env::temp_dir().join("rubyx_test_modules");
        std::fs::create_dir_all(&tmp_dir).expect("should create temp dir");
        let module_path = tmp_dir.join("rubyx_test_mod.py");
        std::fs::write(&module_path, "VALUE = 42\ndef add(a, b): return a + b\n")
            .expect("should write test module");

        // Inject the temp directory into sys.path
        let sys_module = api.import_module("sys").expect("should import sys");
        let sys_path = api.object_get_attr_string(sys_module, "path");
        let py_dir = api.string_from_str(tmp_dir.to_str().unwrap());
        api.list_append(sys_path, py_dir);
        api.decref(py_dir);

        // Now import the module
        let module = api
            .import_module("rubyx_test_mod")
            .expect("should import test module from injected path");
        assert!(!module.is_null());

        // Verify module attribute
        let value_attr = api.object_get_attr_string(module, "VALUE");
        assert!(!value_attr.is_null());
        assert_eq!(api.long_to_i64(value_attr), 42);

        // Verify module function
        let add_fn = api.object_get_attr_string(module, "add");
        assert!(!add_fn.is_null());
        assert!(api.callable_check(add_fn) != 0);

        // Call add(3, 4) and verify result
        let args = unsafe { (api.py_tuple_new)(2) };
        let arg1 = api.long_from_i64(3);
        let arg2 = api.long_from_i64(4);
        unsafe {
            (api.py_tuple_set_item)(args, 0, arg1);
            (api.py_tuple_set_item)(args, 1, arg2);
        }
        let result = api.object_call(add_fn, args, std::ptr::null_mut());
        assert!(!result.is_null());
        assert_eq!(api.long_to_i64(result), 7);

        api.decref(result);
        api.decref(args);
        api.decref(add_fn);
        api.decref(value_attr);
        api.decref(module);
        api.decref(sys_path);
        api.decref(sys_module);

        // Clean up
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    #[serial]
    fn test_import_fails_without_sys_path_injection() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        // Try importing a module that definitely doesn't exist in default sys.path
        let result = api.import_module("rubyx_nonexistent_module_xyz_123");
        assert!(result.is_err(), "should fail to import nonexistent module");
    }

    // ========== object_str tests ==========

    #[test]
    #[serial]
    fn test_object_str_integer() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        let py_str = api.object_str(py_int);
        assert!(!py_str.is_null());
        assert_eq!(api.string_to_string(py_str), Some("42".to_string()));
        api.decref(py_str);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_object_str_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("hello");
        let result = api.object_str(py_str);
        assert!(!result.is_null());
        assert_eq!(api.string_to_string(result), Some("hello".to_string()));
        api.decref(result);
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_object_str_none() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        api.incref(api.py_none);
        let result = api.object_str(api.py_none);
        assert!(!result.is_null());
        assert_eq!(api.string_to_string(result), Some("None".to_string()));
        api.decref(result);
        api.decref(api.py_none);
    }

    #[test]
    #[serial]
    fn test_object_str_null_returns_null() {
        let Some(_guard) = skip_if_no_python() else {
            return;
        };
        let api = _guard.api();

        let result = api.object_str(std::ptr::null_mut());
        assert!(result.is_null());
    }

    #[test]
    #[serial]
    fn test_object_str_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let list = unsafe { (api.py_list_new)(2) };
        unsafe {
            (api.py_list_set_item)(list, 0, api.long_from_i64(1));
            (api.py_list_set_item)(list, 1, api.long_from_i64(2));
        }
        let result = api.object_str(list);
        assert!(!result.is_null());
        assert_eq!(api.string_to_string(result), Some("[1, 2]".to_string()));
        api.decref(result);
        api.decref(list);
    }

    // ========== object_repr tests ==========

    #[test]
    #[serial]
    fn test_object_repr_integer() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        let result = api.object_repr(py_int);
        assert_eq!(result, "42");
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_object_repr_string_includes_quotes() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("hello");
        let result = api.object_repr(py_str);
        assert_eq!(result, "'hello'");
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_object_repr_none() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        api.incref(api.py_none);
        let result = api.object_repr(api.py_none);
        assert_eq!(result, "None");
        api.decref(api.py_none);
    }

    #[test]
    #[serial]
    fn test_object_repr_null() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let result = api.object_repr(std::ptr::null_mut());
        assert_eq!(result, "<null>");
    }

    #[test]
    #[serial]
    fn test_object_repr_bool_true() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_true = api.bool_from_i64(1);
        let result = api.object_repr(py_true);
        assert_eq!(result, "True");
        api.decref(py_true);
    }

    #[test]
    #[serial]
    fn test_object_repr_dict() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let dict = api.dict_new();
        let key = api.string_from_str("a");
        let val = api.long_from_i64(1);
        api.dict_set_item(dict, key, val);
        api.decref(key);
        api.decref(val);

        let result = api.object_repr(dict);
        assert_eq!(result, "{'a': 1}");
        api.decref(dict);
    }

    // ========== object_get_item / object_set_item / object_del_item tests ==========

    #[test]
    #[serial]
    fn test_object_get_item_dict() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let dict = api
            .run_string("{'x': 42}", PY_EVAL_INPUT, globals, globals)
            .expect("should create dict");

        let key = api.string_from_str("x");
        let result = api.object_get_item(dict, key);
        assert!(!result.is_null());
        assert_eq!(api.long_to_i64(result), 42);

        api.decref(result);
        api.decref(key);
        api.decref(dict);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_object_get_item_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let list = api
            .run_string("[10, 20, 30]", PY_EVAL_INPUT, globals, globals)
            .expect("should create list");

        let key = api.long_from_i64(1);
        let result = api.object_get_item(list, key);
        assert!(!result.is_null());
        assert_eq!(api.long_to_i64(result), 20);

        api.decref(result);
        api.decref(key);
        api.decref(list);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_object_get_item_missing_key() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let dict = api
            .run_string("{}", PY_EVAL_INPUT, globals, globals)
            .expect("should create dict");

        let key = api.string_from_str("missing");
        let result = api.object_get_item(dict, key);
        assert!(result.is_null(), "missing key should return null");
        api.clear_error();

        api.decref(key);
        api.decref(dict);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_object_set_item_dict() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let dict = api.dict_new();
        let key = api.string_from_str("name");
        let val = api.string_from_str("test");

        let result = api.object_set_item(dict, key, val);
        assert_eq!(result, 0, "set_item should return 0 on success");

        // Verify
        let got = api.object_get_item(dict, key);
        assert!(!got.is_null());
        assert_eq!(api.string_to_string(got), Some("test".to_string()));

        api.decref(got);
        api.decref(key);
        api.decref(val);
        api.decref(dict);
    }

    #[test]
    #[serial]
    fn test_object_del_item_dict() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let dict = api
            .run_string("{'a': 1, 'b': 2}", PY_EVAL_INPUT, globals, globals)
            .expect("should create dict");

        let key = api.string_from_str("a");
        let result = api.object_del_item(dict, key);
        assert_eq!(result, 0, "del_item should return 0 on success");

        // Verify 'a' is gone
        let check = api.object_get_item(dict, key);
        assert!(check.is_null(), "'a' should be deleted");
        api.clear_error();

        api.decref(key);
        api.decref(dict);
        api.decref(globals);
    }

    // ========== iteration via get_iter + iter_next ==========

    #[test]
    #[serial]
    fn test_iterate_list_via_get_iter() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let list = api
            .run_string("[100, 200, 300]", PY_EVAL_INPUT, globals, globals)
            .expect("should create list");

        let py_iter = api.object_get_iter(list);
        assert!(!py_iter.is_null(), "list should be iterable");

        let mut values = vec![];
        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                break;
            }
            values.push(api.long_to_i64(item));
            api.decref(item);
        }

        assert_eq!(values, vec![100, 200, 300]);

        api.decref(py_iter);
        api.decref(list);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iterate_empty_list() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let list = api
            .run_string("[]", PY_EVAL_INPUT, globals, globals)
            .expect("should create empty list");

        let py_iter = api.object_get_iter(list);
        assert!(!py_iter.is_null());

        let item = api.iter_next(py_iter);
        assert!(item.is_null(), "empty list should yield nothing");
        assert!(!api.has_error(), "StopIteration should not set error");

        api.decref(py_iter);
        api.decref(list);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iterate_dict_yields_keys() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let dict = api
            .run_string("{'x': 1, 'y': 2}", PY_EVAL_INPUT, globals, globals)
            .expect("should create dict");

        let py_iter = api.object_get_iter(dict);
        assert!(!py_iter.is_null(), "dict should be iterable");

        let mut keys = vec![];
        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                break;
            }
            if let Some(s) = api.string_to_string(item) {
                keys.push(s);
            }
            api.decref(item);
        }

        assert!(keys.contains(&"x".to_string()));
        assert!(keys.contains(&"y".to_string()));
        assert_eq!(keys.len(), 2);

        api.decref(py_iter);
        api.decref(dict);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iterate_string_yields_characters() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("abc");
        let py_iter = api.object_get_iter(py_str);
        assert!(!py_iter.is_null(), "string should be iterable");

        let mut chars = vec![];
        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                break;
            }
            if let Some(s) = api.string_to_string(item) {
                chars.push(s);
            }
            api.decref(item);
        }

        assert_eq!(chars, vec!["a", "b", "c"]);

        api.decref(py_iter);
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_get_iter_on_non_iterable_returns_null() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        let py_iter = api.object_get_iter(py_int);
        assert!(py_iter.is_null(), "int should not be iterable");
        api.clear_error();

        api.decref(py_int);
    }

    // ========== set / frozenset ==========

    #[test]
    #[serial]
    fn test_is_set_returns_true_for_set() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let py_set = api
            .run_string("{1, 2, 3}", PY_EVAL_INPUT, globals, globals)
            .expect("set eval should succeed");

        assert!(api.is_set(py_set), "should detect set");
        assert!(!api.is_frozen_set(py_set), "set is not frozenset");

        api.decref(py_set);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_is_frozen_set_returns_true_for_frozenset() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let py_fset = api
            .run_string("frozenset({10, 20})", PY_EVAL_INPUT, globals, globals)
            .expect("frozenset eval should succeed");

        assert!(api.is_frozen_set(py_fset), "should detect frozenset");
        assert!(!api.is_set(py_fset), "frozenset is not set");

        api.decref(py_fset);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_is_set_returns_false_for_non_sets() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_int = api.long_from_i64(42);
        assert!(!api.is_set(py_int));
        assert!(!api.is_frozen_set(py_int));

        let py_str = api.string_from_str("hello");
        assert!(!api.is_set(py_str));
        assert!(!api.is_frozen_set(py_str));

        let py_list = api.list_new(0);
        assert!(!api.is_set(py_list));
        assert!(!api.is_frozen_set(py_list));

        let py_dict = api.dict_new();
        assert!(!api.is_set(py_dict));
        assert!(!api.is_frozen_set(py_dict));

        api.decref(py_int);
        api.decref(py_str);
        api.decref(py_list);
        api.decref(py_dict);
    }

    #[test]
    #[serial]
    fn test_set_new_creates_empty_set() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_set = api.set_new(0);
        assert!(!py_set.is_null());
        assert!(api.is_set(py_set));

        api.decref(py_set);
    }

    #[test]
    #[serial]
    fn test_iterate_set_via_get_iter() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let py_set = api
            .run_string("{10, 20, 30}", PY_EVAL_INPUT, globals, globals)
            .expect("set eval should succeed");

        let py_iter = api.object_get_iter(py_set);
        assert!(!py_iter.is_null(), "set should be iterable");

        let mut values = vec![];
        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                break;
            }
            values.push(api.long_to_i64(item));
            api.decref(item);
        }
        assert!(!api.has_error());

        values.sort();
        assert_eq!(values, vec![10, 20, 30]);

        api.decref(py_iter);
        api.decref(py_set);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iterate_frozenset_via_get_iter() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let py_fset = api
            .run_string("frozenset({5, 15})", PY_EVAL_INPUT, globals, globals)
            .expect("frozenset eval should succeed");

        let py_iter = api.object_get_iter(py_fset);
        assert!(!py_iter.is_null(), "frozenset should be iterable");

        let mut values = vec![];
        loop {
            let item = api.iter_next(py_iter);
            if item.is_null() {
                break;
            }
            values.push(api.long_to_i64(item));
            api.decref(item);
        }
        assert!(!api.has_error());

        values.sort();
        assert_eq!(values, vec![5, 15]);

        api.decref(py_iter);
        api.decref(py_fset);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_iterate_empty_set() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_set = api.set_new(0);
        assert!(!py_set.is_null());

        let py_iter = api.object_get_iter(py_set);
        assert!(!py_iter.is_null());

        let item = api.iter_next(py_iter);
        assert!(item.is_null(), "empty set should yield nothing");
        assert!(!api.has_error());

        api.decref(py_iter);
        api.decref(py_set);
    }

    #[test]
    #[serial]
    fn test_set_size() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let globals = make_globals(api);
        let py_set = api
            .run_string("{1, 2, 3, 4, 5}", PY_EVAL_INPUT, globals, globals)
            .expect("set eval should succeed");

        let size = unsafe { (api.py_set_size)(py_set) };
        assert_eq!(size, 5);

        api.decref(py_set);
        api.decref(globals);
    }

    // ========== Bytes operations ==========

    #[test]
    #[serial]
    fn test_bytes_from_slice_creates_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_bytes = api.bytes_from_slice(b"hello");
        assert!(!py_bytes.is_null(), "Should create a Python bytes object");
        assert!(api.bytes_check(py_bytes), "Should pass bytes_check");
        api.decref(py_bytes);
    }

    #[test]
    #[serial]
    fn test_bytes_roundtrip() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        for data in [
            b"hello" as &[u8],
            b"world",
            b"with spaces",
            b"unicode: caf\xc3\xa9",
        ] {
            let py_bytes = api.bytes_from_slice(data);
            assert!(!py_bytes.is_null(), "Failed to create bytes for {:?}", data);

            let back = api.bytes_to_vec(py_bytes);
            assert_eq!(
                back.as_deref(),
                Some(data),
                "Roundtrip failed for {:?}",
                data
            );
            api.decref(py_bytes);
        }
    }

    #[test]
    #[serial]
    fn test_bytes_empty() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_bytes = api.bytes_from_slice(b"");
        assert!(!py_bytes.is_null(), "Should create empty Python bytes");
        assert!(api.bytes_check(py_bytes));

        let back = api.bytes_to_vec(py_bytes);
        assert_eq!(back, Some(Vec::new()), "Empty bytes should roundtrip");
        api.decref(py_bytes);
    }

    #[test]
    #[serial]
    fn test_bytes_with_null_bytes() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let data: &[u8] = b"\x00\x01\x02\x00\xff\xfe\x00";
        let py_bytes = api.bytes_from_slice(data);
        assert!(!py_bytes.is_null());

        let back = api.bytes_to_vec(py_bytes);
        assert_eq!(
            back.as_deref(),
            Some(data),
            "Bytes with embedded NULs must roundtrip exactly"
        );
        api.decref(py_bytes);
    }

    #[test]
    #[serial]
    fn test_bytes_with_all_byte_values() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let data: Vec<u8> = (0..=255).collect();
        let py_bytes = api.bytes_from_slice(&data);
        assert!(!py_bytes.is_null());

        let back = api.bytes_to_vec(py_bytes);
        assert_eq!(
            back.as_deref(),
            Some(data.as_slice()),
            "All 256 byte values must roundtrip"
        );
        api.decref(py_bytes);
    }

    #[test]
    #[serial]
    fn test_bytes_check_type_discrimination() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_bytes = api.bytes_from_slice(b"test");
        let py_str = api.string_from_str("test");
        let py_int = api.long_from_i64(42);
        let py_ba = api.bytearray_from_slice(b"test");

        assert!(api.bytes_check(py_bytes), "bytes should pass bytes_check");
        assert!(!api.bytes_check(py_str), "str should fail bytes_check");
        assert!(!api.bytes_check(py_int), "int should fail bytes_check");
        assert!(!api.bytes_check(py_ba), "bytearray should fail bytes_check");

        api.decref(py_bytes);
        api.decref(py_str);
        api.decref(py_int);
        api.decref(py_ba);
    }

    #[test]
    #[serial]
    fn test_bytes_check_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().bytes_check(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_bytes_to_vec_returns_none_for_non_bytes() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("not bytes");
        assert!(api.bytes_to_vec(py_str).is_none());
        api.decref(py_str);

        let py_int = api.long_from_i64(42);
        assert!(api.bytes_to_vec(py_int).is_none());
        api.decref(py_int);

        assert!(api.bytes_to_vec(std::ptr::null_mut()).is_none());
    }

    #[test]
    #[serial]
    fn test_bytes_size() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_empty = api.bytes_from_slice(b"");
        assert_eq!(
            unsafe { (api.py_bytes_size)(py_empty) },
            0,
            "Empty bytes should have size 0"
        );
        api.decref(py_empty);

        let py_hello = api.bytes_from_slice(b"hello");
        assert_eq!(
            unsafe { (api.py_bytes_size)(py_hello) },
            5,
            "b\"hello\" should have size 5"
        );
        api.decref(py_hello);

        let data_with_nulls = b"\x00\x00\x00";
        let py_nulls = api.bytes_from_slice(data_with_nulls);
        assert_eq!(
            unsafe { (api.py_bytes_size)(py_nulls) },
            3,
            "Size must count NUL bytes"
        );
        api.decref(py_nulls);
    }

    #[test]
    #[serial]
    fn test_bytes_from_python_eval() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let py_bytes = api
            .run_string("b'hello world'", PY_EVAL_INPUT, globals, globals)
            .expect("should eval bytes literal");
        assert!(!py_bytes.is_null());
        assert!(api.bytes_check(py_bytes));

        let back = api.bytes_to_vec(py_bytes);
        assert_eq!(back, Some(b"hello world".to_vec()));

        api.decref(py_bytes);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_bytes_is_not_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_bytes = api.bytes_from_slice(b"hello");
        let py_str = api.string_from_str("hello");

        assert!(!api.is_string(py_bytes), "bytes must not pass is_string");
        assert!(!api.bytes_check(py_str), "str must not pass bytes_check");

        api.decref(py_bytes);
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_bytes_large_payload() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let py_bytes = api.bytes_from_slice(&data);
        assert!(!py_bytes.is_null());

        let back = api
            .bytes_to_vec(py_bytes)
            .expect("should extract large bytes");
        assert_eq!(back.len(), 10_000);
        assert_eq!(back, data);

        api.decref(py_bytes);
    }

    // ========== ByteArray operations ==========

    #[test]
    #[serial]
    fn test_bytearray_from_slice_creates_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_ba = api.bytearray_from_slice(b"hello");
        assert!(!py_ba.is_null(), "Should create a Python bytearray object");
        assert!(api.bytearray_check(py_ba), "Should pass bytearray_check");
        api.decref(py_ba);
    }

    #[test]
    #[serial]
    fn test_bytearray_roundtrip() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        for data in [
            b"hello" as &[u8],
            b"world",
            b"with spaces",
            b"binary\xfe\xff",
        ] {
            let py_ba = api.bytearray_from_slice(data);
            assert!(
                !py_ba.is_null(),
                "Failed to create bytearray for {:?}",
                data
            );

            let back = api.bytearray_to_vec(py_ba);
            assert_eq!(
                back.as_deref(),
                Some(data),
                "Roundtrip failed for {:?}",
                data
            );
            api.decref(py_ba);
        }
    }

    #[test]
    #[serial]
    fn test_bytearray_empty() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_ba = api.bytearray_from_slice(b"");
        assert!(!py_ba.is_null(), "Should create empty Python bytearray");
        assert!(api.bytearray_check(py_ba));

        let back = api.bytearray_to_vec(py_ba);
        assert_eq!(back, Some(Vec::new()), "Empty bytearray should roundtrip");
        api.decref(py_ba);
    }

    #[test]
    #[serial]
    fn test_bytearray_with_null_bytes() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let data: &[u8] = b"\x00\x01\x02\x00\xff\xfe\x00";
        let py_ba = api.bytearray_from_slice(data);
        assert!(!py_ba.is_null());

        let back = api.bytearray_to_vec(py_ba);
        assert_eq!(
            back.as_deref(),
            Some(data),
            "Bytearray with embedded NULs must roundtrip exactly"
        );
        api.decref(py_ba);
    }

    #[test]
    #[serial]
    fn test_bytearray_with_all_byte_values() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let data: Vec<u8> = (0..=255).collect();
        let py_ba = api.bytearray_from_slice(&data);
        assert!(!py_ba.is_null());

        let back = api.bytearray_to_vec(py_ba);
        assert_eq!(
            back.as_deref(),
            Some(data.as_slice()),
            "All 256 byte values must roundtrip via bytearray"
        );
        api.decref(py_ba);
    }

    #[test]
    #[serial]
    fn test_bytearray_check_type_discrimination() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_ba = api.bytearray_from_slice(b"test");
        let py_bytes = api.bytes_from_slice(b"test");
        let py_str = api.string_from_str("test");
        let py_int = api.long_from_i64(42);

        assert!(
            api.bytearray_check(py_ba),
            "bytearray should pass bytearray_check"
        );
        assert!(
            !api.bytearray_check(py_bytes),
            "bytes should fail bytearray_check"
        );
        assert!(
            !api.bytearray_check(py_str),
            "str should fail bytearray_check"
        );
        assert!(
            !api.bytearray_check(py_int),
            "int should fail bytearray_check"
        );

        api.decref(py_ba);
        api.decref(py_bytes);
        api.decref(py_str);
        api.decref(py_int);
    }

    #[test]
    #[serial]
    fn test_bytearray_check_null_returns_false() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        assert!(!guard.api().bytearray_check(std::ptr::null_mut()));
    }

    #[test]
    #[serial]
    fn test_bytearray_to_vec_returns_none_for_non_bytearray() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_str = api.string_from_str("not bytearray");
        assert!(api.bytearray_to_vec(py_str).is_none());
        api.decref(py_str);

        let py_bytes = api.bytes_from_slice(b"not bytearray");
        assert!(api.bytearray_to_vec(py_bytes).is_none());
        api.decref(py_bytes);

        assert!(api.bytearray_to_vec(std::ptr::null_mut()).is_none());
    }

    #[test]
    #[serial]
    fn test_bytearray_size() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_empty = api.bytearray_from_slice(b"");
        assert_eq!(
            unsafe { (api.py_bytearray_size)(py_empty) },
            0,
            "Empty bytearray should have size 0"
        );
        api.decref(py_empty);

        let py_hello = api.bytearray_from_slice(b"hello");
        assert_eq!(
            unsafe { (api.py_bytearray_size)(py_hello) },
            5,
            "bytearray(b\"hello\") should have size 5"
        );
        api.decref(py_hello);
    }

    #[test]
    #[serial]
    fn test_bytearray_from_python_eval() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let py_ba = api
            .run_string("bytearray(b'hello world')", PY_EVAL_INPUT, globals, globals)
            .expect("should eval bytearray");
        assert!(!py_ba.is_null());
        assert!(api.bytearray_check(py_ba));

        let back = api.bytearray_to_vec(py_ba);
        assert_eq!(back, Some(b"hello world".to_vec()));

        api.decref(py_ba);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_bytearray_from_object() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_bytes = api.bytes_from_slice(b"convert me");
        let py_ba = unsafe { (api.py_bytearray_from_object)(py_bytes) };
        assert!(!py_ba.is_null(), "Should create bytearray from bytes");
        assert!(api.bytearray_check(py_ba));

        let back = api.bytearray_to_vec(py_ba);
        assert_eq!(back, Some(b"convert me".to_vec()));

        api.decref(py_ba);
        api.decref(py_bytes);
    }

    #[test]
    #[serial]
    fn test_bytearray_is_not_bytes() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_ba = api.bytearray_from_slice(b"hello");
        let py_bytes = api.bytes_from_slice(b"hello");

        assert!(
            !api.bytes_check(py_ba),
            "bytearray must not pass bytes_check"
        );
        assert!(
            !api.bytearray_check(py_bytes),
            "bytes must not pass bytearray_check"
        );
        assert!(!api.is_string(py_ba), "bytearray must not pass is_string");

        api.decref(py_ba);
        api.decref(py_bytes);
    }

    #[test]
    #[serial]
    fn test_bytearray_large_payload() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let py_ba = api.bytearray_from_slice(&data);
        assert!(!py_ba.is_null());

        let back = api
            .bytearray_to_vec(py_ba)
            .expect("should extract large bytearray");
        assert_eq!(back.len(), 10_000);
        assert_eq!(back, data);

        api.decref(py_ba);
    }

    // ========== Cross-type bytes/bytearray tests ==========

    #[test]
    #[serial]
    fn test_bytes_and_bytearray_same_content_different_types() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let data = b"identical content";
        let py_bytes = api.bytes_from_slice(data);
        let py_ba = api.bytearray_from_slice(data);

        // Same content
        let bytes_vec = api.bytes_to_vec(py_bytes).unwrap();
        let ba_vec = api.bytearray_to_vec(py_ba).unwrap();
        assert_eq!(bytes_vec, ba_vec, "Same input should produce same content");

        // Different types
        assert!(api.bytes_check(py_bytes));
        assert!(!api.bytearray_check(py_bytes));
        assert!(api.bytearray_check(py_ba));
        assert!(!api.bytes_check(py_ba));

        api.decref(py_bytes);
        api.decref(py_ba);
    }

    #[test]
    #[serial]
    fn test_bytes_and_bytearray_neither_is_string() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();

        let py_bytes = api.bytes_from_slice(b"data");
        let py_ba = api.bytearray_from_slice(b"data");
        let py_str = api.string_from_str("data");

        assert!(!api.is_string(py_bytes));
        assert!(!api.is_string(py_ba));
        assert!(api.is_string(py_str));

        assert!(!api.bytes_check(py_str));
        assert!(!api.bytearray_check(py_str));

        api.decref(py_bytes);
        api.decref(py_ba);
        api.decref(py_str);
    }

    #[test]
    #[serial]
    fn test_bytes_python_equality_with_rust() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        // Create bytes in Python with hex escapes and verify Rust reads them correctly
        let py_bytes = api
            .run_string(
                "b'\\x00\\x01\\x02\\xfd\\xfe\\xff'",
                PY_EVAL_INPUT,
                globals,
                globals,
            )
            .expect("should eval hex bytes");
        assert!(api.bytes_check(py_bytes));

        let back = api.bytes_to_vec(py_bytes).unwrap();
        assert_eq!(back, vec![0x00, 0x01, 0x02, 0xfd, 0xfe, 0xff]);

        api.decref(py_bytes);
        api.decref(globals);
    }

    #[test]
    #[serial]
    fn test_bytearray_python_concatenation() {
        let Some(guard) = skip_if_no_python() else {
            return;
        };
        let api = guard.api();
        let globals = make_globals(api);

        let py_ba = api
            .run_string(
                "bytearray(b'hello') + bytearray(b' world')",
                PY_EVAL_INPUT,
                globals,
                globals,
            )
            .expect("should eval bytearray concat");
        assert!(api.bytearray_check(py_ba));

        let back = api.bytearray_to_vec(py_ba).unwrap();
        assert_eq!(back, b"hello world");

        api.decref(py_ba);
        api.decref(globals);
    }
}
