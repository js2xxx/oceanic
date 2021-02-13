#![no_std]
#![feature(asm)]
#![feature(lang_items)]

mod log;
use ::log as l;

#[no_mangle]
pub extern "C" fn kmain() {
      self::log::init();
      l::info!("kmain: Starting initialization");
      loop {
            unsafe { asm!("pause") }
      }
}

#[panic_handler]
fn panic_handler(_info: &core::panic::PanicInfo) -> ! {
      loop {
            unsafe { asm!("pause") };
      }
}

#[lang = "eh_personality"]
pub extern "C" fn rust_eh_personality() {}

#[allow(non_snake_case)]
#[no_mangle]
/// Required to handle panics.
pub extern "C" fn _Unwind_Resume() -> ! {
      unsafe { archop::halt_loop(None) }
}
