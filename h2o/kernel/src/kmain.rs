#![no_std]
#![no_main]
#![allow(unused_unsafe)]
#![warn(clippy::missing_errors_doc)]
#![warn(clippy::missing_panics_doc)]
#![feature(alloc_layout_extra)]
#![feature(alloc_error_handler)]
#![feature(allocator_api)]
#![feature(asm)]
#![feature(bool_to_option)]
#![feature(box_into_inner)]
#![feature(box_syntax)]
#![feature(c_variadic)]
#![feature(concat_idents)]
#![feature(const_btree_new)]
#![feature(const_fn_fn_ptr_basics)]
#![feature(const_fn_trait_bound)]
#![feature(linked_list_remove)]
#![feature(map_first_last)]
#![feature(new_uninit)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(result_flattening)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]
#![feature(trace_macros)]
#![feature(unzip_option)]
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

static KARGS: Lazy<minfo::KernelArgs> =
    Lazy::new(|| unsafe { (minfo::KARGS_BASE as *const minfo::KernelArgs).read() });

#[no_mangle]
pub extern "C" fn kmain() {
    unsafe { cpu::set_id(true) };

    // SAFE: Everything is uninitialized.
    unsafe { self::log::init(l::Level::Debug) };
    l::info!("Starting the kernel");

    mem::init();

    unsafe { mem::space::init_bsp_early() };
    unsafe { cpu::arch::init_bsp_early() };
    sched::task::tid::init();

    unsafe { mem::space::init() };
    unsafe { cpu::arch::init() };

    unsafe { dev::init_intr_chip() };

    sched::init();

    // Test end
    l::debug!("Reaching end of kernel");
}

#[no_mangle]
pub extern "C" fn kmain_ap() {
    unsafe { cpu::set_id(false) };
    l::debug!("Starting the kernel");

    unsafe { mem::space::init() };
    unsafe { cpu::arch::init_ap() };

    sched::init();

    l::debug!("Finished");
    unsafe { archop::halt_loop(Some(true)) };
}
