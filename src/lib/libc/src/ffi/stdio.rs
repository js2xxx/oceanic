use core::ffi::*;

#[repr(transparent)]
pub struct FILE(u32);

#[allow(non_upper_case_globals)]
pub const stderr: u32 = 0;

pub const SEEK_SET: c_int = 0;

#[no_mangle]
pub extern "C" fn fflush(_stream: *mut FILE) -> c_int {
    todo!()
}

#[no_mangle]
pub extern "C" fn fclose(_stream: *mut FILE) -> c_int {
    todo!()
}

#[no_mangle]
pub extern "C" fn fopen(_name: *const c_char, _mode: *const c_char) -> *mut FILE {
    todo!()
}

#[no_mangle]
pub extern "C" fn fread(
    _buf: *mut c_void,
    _size: usize,
    _count: usize,
    _stream: *mut FILE,
) -> usize {
    todo!()
}

#[no_mangle]
pub extern "C" fn fwrite(
    _buf: *const c_void,
    _size: usize,
    _count: usize,
    _stream: *mut FILE,
) -> usize {
    todo!()
}

#[no_mangle]
pub extern "C" fn fseek(_stream: *mut FILE, _offset: c_long, _origin: c_int) -> c_int {
    todo!()
}

/// # Safety
///
/// The caller must ensure that `args` corresponds to the placeholders in `fmt`,
/// which is required to be a valid format c-string.
#[no_mangle]
pub unsafe extern "C" fn vfprintf(_stream: *mut FILE, _fmt: *const c_char, _args: VaList) -> c_int {
    todo!()
}
