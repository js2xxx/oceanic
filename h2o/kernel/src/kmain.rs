#![no_std]
#![allow(unused_unsafe)]
#![warn(clippy::missing_errors_doc)]
#![warn(clippy::missing_panics_doc)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(asm)]
#![feature(bool_to_option)]
#![feature(box_into_inner)]
#![feature(box_syntax)]
#![feature(concat_idents)]
#![feature(const_btree_new)]
#![feature(const_fn_fn_ptr_basics)]
#![feature(const_fn_trait_bound)]
#![feature(default_alloc_error_handler)]
#![feature(lang_items)]
#![feature(linked_list_remove)]
#![feature(map_first_last)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]
#![feature(trace_macros)]
#![feature(vec_into_raw_parts)]

pub mod cpu;
pub mod dev;
pub mod log;
pub mod mem;
pub mod rxx;
pub mod sched;
pub mod syscall;

use ::log as l;
use spin::Lazy;

extern crate alloc;

static KARGS: Lazy<kargs::KernelArgs> =
    Lazy::new(|| unsafe { (minfo::KARGS_BASE as *const kargs::KernelArgs).read() });

#[no_mangle]
pub extern "C" fn kmain() {
    unsafe { cpu::set_id(true) };

    // SAFE: Everything is uninitialized.
    unsafe { self::log::init(l::Level::Debug) };
    l::info!("Starting initialization");

    mem::init();

    l::debug!("Creating the kernel space");
    unsafe { mem::space::init_bsp_early() };
    sched::task::tid::init();

    l::debug!("Initializing ACPI tables");
    unsafe { dev::acpi::init_tables(*KARGS.rsdp) };
    let lapic_data =
        unsafe { dev::acpi::table::get_lapic_data() }.expect("Failed to get LAPIC data");
    let ioapic_data =
        unsafe { dev::acpi::table::get_ioapic_data() }.expect("Failed to get IOAPIC data");

    l::debug!("Set up CPU architecture");
    unsafe { cpu::arch::init(lapic_data) };
    unsafe { mem::space::init() };

    l::debug!("Set up Interrupt system");
    unsafe { dev::init_intr_chip(ioapic_data) };

    l::debug!("Set up tasks");
    sched::init();

    // Tests
    // let hpet_data =
    //       unsafe { dev::acpi::table::hpet::get_hpet_data().expect("Failed to get
    // HPET data") }; let hpet = unsafe { dev::hpet::Hpet::new(hpet_data)
    // }.expect("Failed to initialize HPET"); let _ = core::mem::ManuallyDrop::
    // new(hpet);

    // Test end
    l::debug!("Reaching end of kernel");
}

#[no_mangle]
pub extern "C" fn kmain_ap() {
    unsafe { cpu::set_id(false) };

    l::debug!("Starting initialization");
    unsafe { mem::space::init() };

    l::debug!("Set up CPU architecture");
    let lapic_data =
        unsafe { dev::acpi::table::get_lapic_data() }.expect("Failed to get LAPIC data");
    unsafe { cpu::arch::init_ap(lapic_data) };

    l::debug!("Set up tasks");
    sched::init();

    l::debug!("Finished");
    unsafe { archop::halt_loop(Some(true)) };
}
