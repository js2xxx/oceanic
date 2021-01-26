#![no_std]
#![feature(asm)]
#![feature(lang_items)]

#[no_mangle]
pub extern "C" fn kmain() {}

#[panic_handler]
fn panic_handler(_info: &core::panic::PanicInfo) -> ! {
      loop {
            unsafe { asm!("pause") };
      }
}

#[lang = "eh_personality"]
pub extern "C" fn rust_eh_personality() {}
