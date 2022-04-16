#![no_std]
#![warn(clippy::missing_panics_doc)]
#![feature(allocator_api)]
#![feature(lang_items)]
#![feature(linkage)]

pub mod call;
pub mod error;
pub mod feat;
pub mod ipc;
pub mod mem;
pub mod res;
#[cfg(feature = "stub")]
pub mod stub;
pub mod task;

pub use sv_gen::*;

#[cfg(all(not(feature = "stub"), feature = "call"))]
pub use self::call::*;
#[cfg(feature = "stub")]
pub use self::stub::*;
pub use self::{
    call::{hdl::Handle, reg::*},
    error::*,
    feat::*,
};

#[cfg(all(not(feature = "call"), feature = "vdso"))]
compile_error!("The VDSO feature is onlye supported with call feature");

#[cfg(feature = "vdso")]
#[panic_handler]
#[linkage = "weak"]
#[no_mangle]
pub extern "C" fn rust_begin_unwind(_: &core::panic::PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}
