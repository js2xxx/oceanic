#![no_std]
#![feature(asm)]
#![feature(default_alloc_error_handler)]
#![feature(lang_items)]

mod log;
mod mem;

use ::log as l;

#[no_mangle]
pub extern "C" fn kmain(
      rsdp: *const core::ffi::c_void,
      efi_mmap_paddr: paging::PAddr,
      efi_mmap_len: usize,
      efi_mmap_unit: usize,
      tls_size: usize,
) {
      self::log::init(l::Level::Debug);
      l::info!("kmain: Starting initialization");

      mem::init(efi_mmap_paddr, efi_mmap_len, efi_mmap_unit);
      pmm::dump_data(pmm::PFType::Max);
      loop {
            unsafe { asm!("pause") }
      }
}

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
      l::error!("Kernel {}", info);
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
