#![no_std]
#![no_main]
#![feature(alloc_layout_extra)]
#![feature(box_syntax)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]

mod mem;

extern crate alloc;

pub use solvent::rxx::*;

#[no_mangle]
extern "C" fn tmain(_: solvent::Handle) {
    solvent::log::init(log::Level::Debug);
    log::info!("Starting initialization");
    mem::init();

    solvent::test();

    log::debug!("Reaching end of TINIT");
}
