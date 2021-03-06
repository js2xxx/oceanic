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
use solvent::prelude::{Channel, Object, Phys};
pub use svrt::*;

pub use self::rxx::{dynamic, load_address, vdso_map};

fn dl_main(init_chan: Channel) -> rxx::DlReturn {
    dso::init().expect("Failed to initialize the DSO list");
    dbglog::init(log::Level::Debug);

    let startup_args = init_chan
        .receive::<StartupArgs>()
        .expect("Failed to receive boot message");

    let _args = init_rt(startup_args).expect("Failed to initialize runtime");

    let prog = take_startup_handle(HandleInfo::new().with_handle_type(HandleType::ProgramPhys));
    let prog = unsafe { Phys::from_raw(prog) };

    let elf =
        dso::Dso::load(&prog, cstr!("<PROGRAM>").into(), true).expect("Failed to load program");

    log::debug!("Reaching end of the dynamic linker");

    rxx::DlReturn {
        entry: elf.entry,
        init_chan: Channel::into_raw(init_chan),
    }
}

#[no_mangle]
extern "C" fn __libc_start_init() {
    dso::dso_list().lock().do_init();
}

#[no_mangle]
extern "C" fn __libc_exit_fini() {
    dso::dso_list().lock().do_fini();
}
