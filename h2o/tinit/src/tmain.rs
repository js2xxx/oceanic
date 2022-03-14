#![no_std]
#![no_main]
#![allow(unused_unsafe)]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(box_syntax)]
#![feature(lang_items)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]

mod log;
mod mem;
mod rxx;
mod test;

use solvent::{ipc::Channel, obj::Object};

extern crate alloc;

#[no_mangle]
extern "C" fn tmain(init_chan: sv_call::Handle) {
    log::init(::log::Level::Debug);
    ::log::info!("Starting initialization");
    mem::init();

    unsafe { test::test_syscall() };

    let init_chan = unsafe { Channel::from_raw(init_chan) };
    let mut packet = Default::default();
    { init_chan.receive(&mut packet) }.expect("Failed to receive the initial packet");

    ::log::debug!("Reaching end of TINIT");
}
