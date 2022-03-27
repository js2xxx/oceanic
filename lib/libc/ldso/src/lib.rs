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

use solvent::prelude::{Object, Phys};
use svrt::{HandleInfo, HandleType, StartupArgs};

pub use self::rxx::{dynamic, init_channel, load_address, vdso_map};

fn dl_main() -> rxx::DlReturn {
    let list = dso::init().expect("Failed to initialize the DSO list");
    dbglog::init(log::Level::Debug);

    log::debug!("dl_main started");

    let mut startup_args = init_channel()
        .receive::<StartupArgs>()
        .expect("Failed to receive boot message");

    let root_virt = startup_args
        .handles
        .remove(&HandleInfo::new().with_handle_type(HandleType::RootVirt))
        .expect("Failed to get root Virt");
    unsafe { imp_alloc::init(root_virt) };

    let handle = startup_args
        .handles
        .remove(&HandleInfo::new().with_handle_type(HandleType::VdsoPhys))
        .expect("Failed to get VDSO physical object");
    let vdso = unsafe { Phys::from_raw(handle) };

    log::debug!("{:?}", unsafe { vdso.raw() });

    log::debug!("Reaching end of the dynamic linker");

    mem::forget(list);
    mem::forget(vdso);
    loop {
        unsafe { core::arch::asm!("pause") }
    }
}
