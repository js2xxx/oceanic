#![no_std]
#![no_main]
#![allow(unused_unsafe)]
#![warn(clippy::missing_errors_doc)]
#![warn(clippy::missing_panics_doc)]
#![feature(alloc_layout_extra)]
#![feature(alloc_error_handler)]
#![feature(allocator_api)]
#![feature(bool_to_option)]
#![feature(box_into_inner)]
#![feature(box_syntax)]
#![feature(const_btree_new)]
#![feature(const_fn_fn_ptr_basics)]
#![feature(core_intrinsics)]
#![feature(downcast_unchecked)]
#![feature(map_first_last)]
#![feature(new_uninit)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(once_cell)]
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

use core::mem::MaybeUninit;

use ::log as l;

extern crate alloc;

static mut KARGS: MaybeUninit<minfo::KernelArgs> = MaybeUninit::uninit();
#[inline]
fn kargs() -> &'static minfo::KernelArgs {
    unsafe { KARGS.assume_init_ref() }
}

#[no_mangle]
pub extern "C" fn kmain() {
    unsafe {
        KARGS.write(core::ptr::read(
            minfo::KARGS_BASE as *const minfo::KernelArgs,
        ));
        cpu::set_id(true);
        cpu::arch::reload_pls();
    }

    // SAFE: Everything is uninitialized.
    unsafe { self::log::init(l::Level::Debug) };
    l::info!("Starting the kernel");

    mem::init();

    unsafe { cpu::arch::init() };

    unsafe { dev::init_intr_chip() };

    sched::init();

    // Test end
    l::debug!("Reaching end of kernel");
}

pub fn kmain_ap() {
    unsafe { cpu::set_id(false) };
    l::debug!("Starting the kernel");

    unsafe { mem::space::init() };
    unsafe { cpu::arch::init_ap() };

    sched::init();

    l::debug!("Finished");
    unsafe { archop::halt_loop(Some(true)) };
}
