#[no_mangle]
pub extern "C" fn exit(s: i32) -> ! {
    // SAFETY: Clean up the context before _Exit.
    unsafe {
        __libc_exit_fini();
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

#[link(name = "ldso")]
extern "C" {
    fn __libc_exit_fini();
}
