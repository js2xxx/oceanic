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

mod log;
mod mem;
mod rxx;
mod test;

extern crate alloc;

#[no_mangle]
extern "C" fn tmain(_: sv_call::Handle) {
    log::init(::log::Level::Debug);
    ::log::info!("Starting initialization");
    mem::init();

    test::test_syscall();

    ::log::debug!("Reaching end of TINIT");
}
