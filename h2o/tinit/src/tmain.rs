#![no_std]
#![feature(thread_local)]

pub use solvent::rxx::*;

#[no_mangle]
extern "C" fn tmain() {
      solvent::log::init(log::Level::Debug);

      log::debug!("Reaching end of TINIT");
}
