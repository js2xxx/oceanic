use alloc::alloc::Global;
use core::{
    alloc::{Allocator, Layout},
    ffi::*,
    ptr::{self, NonNull},
};

pub const MIN_ALIGN: usize = 0x10;

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
pub unsafe extern "C" fn aligned_free(ptr: *mut c_void, align: usize, size: usize) {
    let layout = Layout::from_size_align(size, align).unwrap();
    let ptr = NonNull::new(ptr.cast::<u8>()).unwrap();
    Global.deallocate(ptr, layout)
}

macro_rules! ok_or_null {
    ($e:expr) => {
        match $e {
            Ok(x) => x,
            Err(_) => return ptr::null_mut(),
        }
    };
}

/// # Safety
///
/// * `ptr` must denote a block of memory via this allocator.
#[no_mangle]
pub unsafe extern "C" fn aligned_realloc(
    ptr: *mut c_void,
    align: usize,
    size: usize,
    new_align: usize,
    new_size: usize,
) -> *mut c_void {
    let ptr = ok_or_null!(NonNull::new(ptr.cast::<u8>()).ok_or(()));
    let layout = ok_or_null!(Layout::from_size_align(size, align));
    let new_layout = ok_or_null!(Layout::from_size_align(new_size, new_align));

    let res = if new_size <= layout.size() {
        Global.shrink(ptr, layout, new_layout)
    } else {
        Global.grow(ptr, layout, new_layout)
    };
    res.map_or(ptr::null_mut(), |ptr| ptr.as_ptr().cast())
}

#[no_mangle]
pub extern "C" fn aligned_alloc(align: usize, size: usize) -> *mut c_void {
    let layout = ok_or_null!(Layout::from_size_align(size, align));

    let res = Global.allocate(layout);
    res.map_or(ptr::null_mut(), |ptr| ptr.as_ptr().cast())
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
