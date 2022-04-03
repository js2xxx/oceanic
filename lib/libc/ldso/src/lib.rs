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
mod rxx;

use cstr_core::cstr;
use solvent::prelude::{Object, Phys};
pub use svrt::*;

pub use self::rxx::{dynamic, init_channel, load_address, vdso_map};

fn dl_main() -> rxx::DlReturn {
    dso::init().expect("Failed to initialize the DSO list");
    dbglog::init(log::Level::Debug);

    let startup_args = init_channel()
        .receive::<StartupArgs>()
        .expect("Failed to receive boot message");

    let _args = init_rt(startup_args).expect("Failed to initialize runtime");

    let prog = take_startup_handle(HandleInfo::new().with_handle_type(HandleType::ProgramPhys));
    let prog = unsafe { Phys::from_raw(prog) };

    let _elf = dso::Dso::load(&prog, cstr!("<Program>")).expect("Failed to load program");

    log::debug!("Reaching end of the dynamic linker");

    loop {
        unsafe { core::arch::asm!("pause") }
    }
}
