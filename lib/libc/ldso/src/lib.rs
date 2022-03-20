#![no_std]
#![feature(alloc_error_handler)]
#![feature(asm_sym)]
#![feature(naked_functions)]

pub mod rxx;

use solvent::prelude::*;

pub use self::rxx::{_start, dynamic_offset, load_address};

fn dl_main(init_chan: Handle, vdso_map: *mut u8) -> rxx::DlReturn {
    unsafe { *(0x12345 as *mut u8) = 0 };
    todo!()
}
