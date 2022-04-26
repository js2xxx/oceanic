#![no_std]
#![warn(clippy::missing_panics_doc)]
#![feature(allocator_api)]
#![feature(asm_const)]
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

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Constants {
    pub ticks_offset: u64,
    pub ticks_multiplier: u128,
    pub ticks_shift: u128,
}

impl Constants {
    pub const fn new() -> Constants {
        Constants {
            ticks_offset: 0,
            ticks_multiplier: 0,
            ticks_shift: 0,
        }
    }
}

#[cfg(feature = "vdso")]
const CONSTANTS_SIZE: usize = core::mem::size_of::<Constants>();
#[cfg(feature = "vdso")]
core::arch::global_asm!("
    .section .rodata
    .global CONSTANTS
    .type CONSTANTS, object
CONSTANTS:
    .fill {CONSTANTS_SIZE}, 1, 0xcc", 
    CONSTANTS_SIZE = const CONSTANTS_SIZE
);

#[cfg(feature = "vdso")]
fn constants() -> Constants {
    let mut addr: *const Constants;

    unsafe {
        core::arch::asm!(
            "lea {}, [rip + CONSTANTS]",
            out(reg) addr
        );
        core::ptr::read(addr)
    }
}

#[cfg(all(not(feature = "call"), feature = "vdso"))]
compile_error!("The VDSO feature is only supported with call feature");

#[cfg(feature = "vdso")]
#[panic_handler]
#[linkage = "weak"]
#[no_mangle]
pub extern "C" fn rust_begin_unwind(_: &core::panic::PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("pause; ud2") }
    }
}
