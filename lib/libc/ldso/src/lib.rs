#![no_std]
#![allow(unused_unsafe)]
#![feature(alloc_error_handler)]
#![feature(asm_sym)]
#![feature(naked_functions)]

mod alloc;
mod dso;
pub mod elf;
pub mod rxx;

pub use self::rxx::{dynamic, init_channel, load_address, vdso_map};

fn dl_main() -> rxx::DlReturn {
    dso::init();
    unsafe {
        *(0x12345 as *mut u8) = 0;
    }
    todo!()
}
