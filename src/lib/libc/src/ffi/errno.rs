use core::ffi::c_int;

/// # Safety
///
/// The caller is responsible for the validity of thread safety access.
#[no_mangle]
pub unsafe extern "C" fn __libc_errno() -> *mut c_int {
    #[thread_local]
    static mut ERRNO: c_int = 0;

    &mut ERRNO
}
