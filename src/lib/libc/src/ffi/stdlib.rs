use core::{ffi::*, ptr};

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
#[no_mangle]
pub unsafe extern "C" fn free(_ptr: *mut c_void) {}

#[no_mangle]
pub extern "C" fn malloc(_size: usize) -> *mut c_void {
    ptr::null_mut()
}

/// # Safety
///
/// The caller must ensure that `name` contains a valid c-string.
#[no_mangle]
pub unsafe extern "C" fn getenv(name: *const c_char) -> *const c_char {
    let name = CStr::from_ptr(name).to_bytes();
    let envs = match svrt::try_get_envs() {
        Ok(envs) => envs,
        Err(_) => return ptr::null(),
    };
    envs.split(|&b| b == 0)
        .filter_map(|s| s.iter().position(|&b| b == b'=').map(|pos| s.split_at(pos)))
        .find_map(|(n, v)| {
            (n == name)
                .then(|| v.split_first())
                .flatten()
                .map(|(_, v)| v)
        })
        .map_or(ptr::null(), |s| s.as_ptr() as _)
}
