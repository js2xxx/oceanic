use alloc::vec::Vec;
use core::{ffi::c_char, panic::PanicInfo};

use solvent::prelude::{Channel, Handle, Object};
use svrt::StartupArgs;

pub type Main =
    unsafe extern "C" fn(argc: u32, argv: *mut *mut c_char, environ: *mut *mut c_char) -> i32;

#[no_mangle]
unsafe extern "C" fn __libc_start_main(init_chan: Handle, main: Main) -> ! {
    let chan = unsafe { Channel::from_raw(init_chan) };
    let args = chan
        .receive::<StartupArgs>()
        .expect("Failed to receive startup args");

    let mut args = svrt::init_rt(args).expect("Failed to initialize runtime");

    let mut argv = args
        .split_inclusive_mut(|&b| b == 0)
        .map(|s| s.as_mut_ptr() as *mut i8)
        .collect::<Vec<_>>();

    let mut environ = svrt::envs()
        .split_inclusive(|&b| b == 0)
        .map(|s| s.as_ptr() as *mut i8)
        .collect::<Vec<_>>();

    __libc_start_init();

    crate::ffi::stdlib::exit(main(
        argv.len() as u32,
        argv.as_mut_ptr(),
        environ.as_mut_ptr(),
    ))
}

#[no_mangle]
pub(crate) extern "C" fn __libc_panic(info: &PanicInfo) -> ! {
    log::error!("{}", info);
    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}

#[link(name = "ldso")]
extern "C" {
    fn __libc_start_init();
    fn __libc_exit_fini();
}
