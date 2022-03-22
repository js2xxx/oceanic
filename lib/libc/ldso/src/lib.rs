#![no_std]
#![allow(unused_unsafe)]
#![feature(alloc_error_handler)]
#![feature(asm_sym)]
#![feature(naked_functions)]
#![feature(int_roundings)]

extern crate alloc;

mod dso;
pub mod elf;
mod imp_alloc;
pub mod rxx;

pub use self::rxx::{dynamic, init_channel, load_address, vdso_map};

fn dl_main() -> rxx::DlReturn {
    dso::init();
    
    unsafe {
        *(0x12345 as *mut u8) = 0;
    }

    let mut boot = Default::default();
    init_channel()
        .receive(&mut boot)
        .expect("Failed to receive boot message");

    todo!()
}
