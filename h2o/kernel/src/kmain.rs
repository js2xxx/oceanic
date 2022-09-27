#![no_std]
#![no_main]
#![allow(unused_unsafe)]
// #![warn(clippy::missing_errors_doc)]
// #![warn(clippy::missing_panics_doc)]
#![allow(clippy::missing_safety_doc)]
#![feature(alloc_layout_extra)]
#![feature(alloc_error_handler)]
#![feature(allocator_api)]
#![feature(assert_matches)]
#![feature(box_into_inner)]
#![feature(box_syntax)]
#![feature(coerce_unsized)]
#![feature(core_intrinsics)]
#![feature(downcast_unchecked)]
#![feature(drain_filter)]
#![feature(int_log)]
#![feature(layout_for_ptr)]
#![feature(linked_list_cursors)]
#![feature(map_first_last)]
#![feature(map_try_insert)]
#![feature(new_uninit)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(once_cell)]
#![feature(ptr_metadata)]
#![feature(receiver_trait)]
#![feature(result_option_inspect)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]
#![feature(trace_macros)]
#![feature(unsize)]
#![feature(unzip_option)]
#![feature(vec_into_raw_parts)]

pub mod cpu;
pub mod dev;
mod log;
pub mod mem;
mod rxx;
pub mod sched;
mod syscall;

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

    // SAFETY: Everything is uninitialized.
    unsafe { self::log::init(l::Level::Debug) };
    l::info!("Starting the kernel");

    mem::init();
    sched::task::init_early();

    unsafe { cpu::arch::init() };

    unsafe { dev::init() };

    sched::init();

    // Test end
    l::trace!("Reaching end of kernel");
}

pub fn kmain_ap() {
    unsafe { cpu::set_id(false) };
    cpu::arch::seg::test_pls();
    l::trace!("Starting the kernel");

    unsafe { mem::space::init() };
    unsafe { cpu::arch::init_ap() };

    sched::init();

    l::trace!("Finished");
    unsafe { archop::halt_loop(Some(true)) };
}
