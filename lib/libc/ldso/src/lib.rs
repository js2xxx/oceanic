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

use core::mem;

pub use self::rxx::{dynamic, init_channel, load_address, vdso_map};

fn dl_main() -> rxx::DlReturn {
    let list = dso::init().expect("Failed to initialize the DSO list");
    dbglog::init(log::Level::Debug);

    log::debug!("dl_main started");

    let mut boot = Default::default();
    init_channel()
        .receive(&mut boot)
        .expect("Failed to receive boot message");
    assert_eq!(&boot.buffer, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

    log::debug!("Reaching end of the dynamic linker");

    mem::forget(list);
    loop {
        unsafe { core::arch::asm!("pause") }
    }
}
