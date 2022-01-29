#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(box_syntax)]
#![feature(lang_items)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]

mod mem;

mod log;
mod rxx;

extern crate alloc;

#[no_mangle]
extern "C" fn tmain(_: sv_call::Handle) {
    log::init(::log::Level::Debug);
    ::log::info!("Starting initialization");
    mem::init();

    sv_call::test();

    ::log::debug!("Reaching end of TINIT");
}
