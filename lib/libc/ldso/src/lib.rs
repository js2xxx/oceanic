#![no_std]
#![feature(alloc_error_handler)]
#![feature(asm_sym)]
#![feature(naked_functions)]

mod elf;
pub mod rxx;

use solvent::prelude::*;

pub use self::rxx::{_start, dynamic, load_address};

pub const DT_RELR: u32 = 36;
pub const DT_RELRSZ: u32 = 35;
pub const DT_RELRENT: u32 = 37;
fn dl_main(init_chan: Handle, vdso_map: *mut u8) -> rxx::DlReturn {
    unsafe { *(0x12345 as *mut u8) = 0 };
    todo!()
}
