#![no_std]
#![feature(asm)]
#![feature(box_syntax)]
#![feature(default_alloc_error_handler)]
#![feature(lang_items)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]

mod log;
mod mem;

use ::log as l;

extern crate alloc;

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

      let _root = mem::range::Range::new(
            paging::LAddr::from(0)..paging::LAddr::from(0x100000),
            mem::range::RangeFlags::all(),
      );
      // let _sub = mem::range::Range::with_parent(
      //       root.clone(),
      //       paging::LAddr::from(0)..paging::LAddr::from(0x100000),
      //       mem::range::RangeFlags::all(),
      // );

      l::debug!("Reaching end of kernel");
}

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
      l::error!("Kernel {}", info);
      unsafe { archop::halt_loop(Some(true)) }
}

#[lang = "eh_personality"]
pub extern "C" fn rust_eh_personality() {}

#[allow(non_snake_case)]
#[no_mangle]
/// Required to handle panics.
pub extern "C" fn _Unwind_Resume() -> ! {
      unsafe { archop::halt_loop(Some(true)) }
}

/// The function indicating memory runs out.
#[lang = "oom"]
fn out_of_memory(layout: core::alloc::Layout) -> ! {
      l::error!("!!!! ALLOCATION ERROR !!!!");
      l::error!("Request: {:?}", layout);

      unsafe { archop::halt_loop(None) };
}
