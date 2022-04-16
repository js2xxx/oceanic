use core::{ffi::c_void, ptr};

#[no_mangle]
pub extern "C" fn exit(s: i32) -> ! {
    // SAFETY: Clean up the context before _Exit.
    unsafe {
        crate::env::__libc_exit_fini();
        _Exit(s)
    }
}

/// # Safety
///
/// This function doesn't clean up the current self-maintained context, and the
/// caller must ensure it is destroyed before calling this function.
#[no_mangle]
pub unsafe extern "C" fn _Exit(s: i32) -> ! {
    solvent::task::exit(s as usize);
}

/// # Safety
///
/// Same as [`_Exit`].
#[no_mangle]
pub unsafe extern "C" fn abort() -> ! {
    panic!("libc::abort()")
}

/// # Safety
///
/// * `ptr` must denote a block of memory via this allocator.
pub unsafe extern "C" fn free(_ptr: *mut c_void) {}

pub extern "C" fn malloc(_size: usize) -> *mut c_void {
    ptr::null_mut()
}
