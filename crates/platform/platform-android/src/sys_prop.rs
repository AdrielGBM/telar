// Reads an Android system property, returning None when unset or on error.
pub fn read_sys_prop(name: &str) -> Option<String> {
    let cname = std::ffi::CString::new(name).ok()?;
    // PROP_VALUE_MAX is 92; keep headroom for the NUL terminator.
    let mut buf = [0u8; 96];
    let n = unsafe {
        libc::__system_property_get(cname.as_ptr(), buf.as_mut_ptr() as *mut libc::c_char)
    };
    if n <= 0 {
        return None;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr() as *const libc::c_char) };
    s.to_str().ok().map(str::to_owned)
}
