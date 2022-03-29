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

use cstr_core::cstr;
use solvent::prelude::{Object, Phys};
use svrt::{HandleInfo, HandleType, StartupArgs};

pub use self::rxx::{dynamic, init_channel, load_address, vdso_map};

fn dl_main() -> rxx::DlReturn {
    let mut list = dso::init().expect("Failed to initialize the DSO list");
    dbglog::init(log::Level::Debug);

    log::debug!("dl_main started");

    let startup_args = init_channel()
        .receive::<StartupArgs>()
        .expect("Failed to receive boot message");

    let _args = svrt::init(startup_args).expect("Failed to initialize runtime");

    let prog =
        svrt::take_startup_handle(HandleInfo::new().with_handle_type(HandleType::ProgramPhys));
    let prog = unsafe { Phys::from_raw(prog) };

    let dso = dso::Dso::load(&prog, cstr!("<Program>")).expect("Failed to load program");
    list.push(dso);

    log::debug!("Reaching end of the dynamic linker");

    mem::forget(list);
    loop {
        unsafe { core::arch::asm!("pause") }
    }
}
