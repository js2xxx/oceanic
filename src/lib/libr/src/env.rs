use alloc::string::{String, ToString};
use core::ffi::CStr;

use crate::rt::ARGS;

pub fn args() -> impl Iterator<Item = String> {
    args_os().map(|s| s.to_str().unwrap().to_string())
}

pub fn args_os() -> impl Iterator<Item = &'static CStr> {
    unsafe {
        ARGS.split_inclusive(|&b| b == 0)
            .map(|s| CStr::from_bytes_with_nul(s).unwrap())
    }
}

#[panic_handler]
fn rust_begin_unwind(info: &core::panic::PanicInfo) -> ! {
    log::error!("{}", info);

    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}
