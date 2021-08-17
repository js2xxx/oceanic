#![no_std]
#![allow(unused_unsafe)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(asm)]
#![feature(box_syntax)]
#![feature(concat_idents)]
#![feature(const_btree_new)]
#![feature(const_fn_fn_ptr_basics)]
#![feature(default_alloc_error_handler)]
#![feature(lang_items)]
#![feature(macro_attributes_in_derive_output)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(ptr_internals)]
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

use kargs::KernelArgs;

use ::log as l;
use minfo::KARGS_BASE;
use spin::Lazy;

extern crate alloc;

static KARGS: Lazy<KernelArgs> = Lazy::new(|| {
      let ptr = KARGS_BASE as *const KernelArgs;
      unsafe { ptr.read() }
});

#[no_mangle]
pub extern "C" fn kmain() {
      unsafe { cpu::set_id() };

      // SAFE: Everything is uninitialized.
      unsafe { self::log::init(l::Level::Debug) };
      l::info!("Starting initialization");

      mem::init();

      l::debug!("Creating the kernel space");
      unsafe { mem::space::init_kernel() };

      l::debug!("Initializing ACPI tables");
      unsafe { dev::acpi::init_tables(*KARGS.rsdp) };
      let lapic_data =
            unsafe { dev::acpi::table::get_lapic_data() }.expect("Failed to get LAPIC data");
      let ioapic_data =
            unsafe { dev::acpi::table::get_ioapic_data() }.expect("Failed to get IOAPIC data");

      l::debug!("Set up CPU architecture");
      unsafe { cpu::arch::init(lapic_data) };

      l::debug!("Set up Interrupt system");
      unsafe { dev::init_intr_chip(ioapic_data) };

      // Tests
      let hpet_data =
            unsafe { dev::acpi::table::hpet::get_hpet_data().expect("Failed to get HPET data") };
      let hpet = unsafe { dev::hpet::Hpet::new(hpet_data) }.expect("Failed to initialize HPET");
      let _ = core::mem::ManuallyDrop::new(hpet);

      // Test end
      l::debug!("Reaching end of kernel");
}

#[no_mangle]
pub extern "C" fn kmain_ap() {
      unsafe { cpu::set_id() };

      l::debug!("Starting initialization");
      unsafe { mem::space::init_ap() };

      let lapic_data =
            unsafe { dev::acpi::table::get_lapic_data() }.expect("Failed to get LAPIC data");

      unsafe { cpu::arch::init_ap(lapic_data) };

      l::debug!("Finished");

      unsafe { archop::halt_loop(Some(false)) };
}
