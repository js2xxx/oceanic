#![no_std]
#![allow(unused_unsafe)]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(int_roundings)]
#![feature(naked_functions)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(result_option_inspect)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]

extern crate alloc;

#[cfg_attr(target_arch = "x86_64", path = "arch/x86_64.rs")]
mod arch;
mod dso;
pub mod elf;
pub mod ffi;
mod imp_alloc;
mod rxx;

use solvent::prelude::{Channel, Object, Phys};
use solvent_core::c_str;
pub use svrt::*;

pub use self::rxx::{dynamic, load_address, vdso_map};

fn dl_main(init_chan: Channel) -> rxx::DlReturn {
    dso::init().expect("Failed to initialize the DSO list");
    dbglog::init(log::Level::Debug);

    let _args = svrt::init_rt(&init_chan).expect("Failed to initialize runtime");

    let prog = take_startup_handle(HandleType::ProgramPhys.into());
    let prog = unsafe { Phys::from_raw(prog) };

    let (elf, _) =
        dso::Dso::load(&prog, c_str!("<PROGRAM>"), true).expect("Failed to load program");

    log::trace!("Reaching end of the dynamic linker");

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
    crate::ffi::__libc_deallocate_tcb();
    dso::dso_list().lock().do_fini();
}
