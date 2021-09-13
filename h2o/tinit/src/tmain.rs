#![no_std]
#![feature(alloc_layout_extra)]
#![feature(box_syntax)]
#![feature(ptr_internals)]
#![feature(slice_ptr_get)]
#![feature(thread_local)]

mod mem;

extern crate alloc;

pub use solvent::rxx::*;

#[no_mangle]
extern "C" fn tmain() {
      solvent::log::init(log::Level::Debug);
      mem::init();

      // log::debug!("Testing solvent::task");
      // solvent::test_task();

      log::debug!("Reaching end of TINIT");
}
