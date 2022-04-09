#[no_mangle]
pub(crate) unsafe extern "C" fn exit(s: i32) -> ! {
    __libc_exit_fini();
    _Exit(s)
}

#[no_mangle]
unsafe extern "C" fn _Exit(s: i32) -> ! {
    solvent::task::exit(s as usize);
}

#[link(name = "ldso")]
extern "C" {
    fn __libc_exit_fini();
}
