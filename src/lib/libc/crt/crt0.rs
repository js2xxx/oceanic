#![no_std]
#![feature(linkage)]

type Main = unsafe extern "C" fn(argc: u32, argv: *mut *mut i8, environ: *mut *mut i8) -> i32;

#[naked]
#[no_mangle]
pub extern "C" fn _start(init_chan: solvent::obj::Handle) -> ! {
    extern "C" {
        fn __libc_start_main(init_chan: solvent::obj::Handle, main: Main) -> !;
        fn main(argc: u32, argv: *mut *mut i8, environ: *mut *mut i8) -> i32;
    }
    unsafe { __libc_start_main(init_chan, main) }
}

#[linkage = "weak"]
#[no_mangle]
pub extern "C" fn __libc_panic(_: &PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}

#[linkage = "weak"]
#[no_mangle]
#[panic_handler]
pub extern "C" fn rust_begin_unwind(info: &core::panic::PanicInfo) -> ! {
    __libc_panic(info)
}
