//! The x86_64 UEFI boot loader for H2O kernel.
//!
//! The H2O's boot loader simply loads the kernel file and binary data for initialization, and then
//! sets up some basic environment variables for it.
//!
//! In order to properly boot H2O, a kernel file and its binary data - initial memory FS is needed.
//!
//! TODO: Add more explanation

#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(alloc_error_handler)]
#![feature(asm)]
#![feature(box_syntax)]
#![feature(maybe_uninit_ref)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(panic_info_message)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(vec_into_raw_parts)]

extern crate alloc;

mod file;
mod mem;
mod outp;
mod rxx;

use core::mem::MaybeUninit;
use log::*;
use static_assertions::const_assert;
use uefi::logger::Logger;
use uefi::prelude::*;
use uefi::table::boot::{EventType, Tpl};

union KMain {
      ptr: *mut u8,
      func: extern "C" fn(
            rsdp: *const core::ffi::c_void,
            efi_mmap_paddr: paging::PAddr,
            tls_size: usize,
      ) -> !,
}
const_assert!(core::mem::size_of::<KMain>() == core::mem::size_of::<*mut u8>());

static mut LOGGER: MaybeUninit<Logger> = MaybeUninit::uninit();

/// Initialize `log` crate for logging messages.
unsafe fn init_log(syst: &SystemTable<Boot>, level: log::LevelFilter) {
      let stdout = syst.stdout();
      LOGGER.as_mut_ptr().write(Logger::new(stdout));
      log::set_logger(LOGGER.assume_init_ref()).expect("Failed to set logger");
      log::set_max_level(level);
}

/// Initialize high-level boot services such as memory, logging and FS.
unsafe fn init_services(img: Handle, syst: &SystemTable<Boot>) {
      /// A callback disabling logging service right before exiting UEFI boot services.
      fn exit_boot_services_callback(_: uefi::Event) {
            log::debug!("Reaching end");
            unsafe { LOGGER.assume_init_mut().disable() };
            uefi::alloc::exit_boot_services();
      }

      let bs = &syst.boot_services();

      uefi::alloc::init(bs);

      if cfg!(debug_assertions) {
            init_log(&syst, log::LevelFilter::Debug);
      } else {
            init_log(&syst, log::LevelFilter::Info);
      }

      bs.create_event(
            EventType::SIGNAL_EXIT_BOOT_SERVICES,
            Tpl::NOTIFY,
            Some(exit_boot_services_callback),
      )
      .expect_success("Failed to subscribe exit_boot_services callback");

      file::init(img, syst);

      mem::init(syst);
}

#[entry]
fn efi_main(img: Handle, syst: SystemTable<Boot>) -> Status {
      unsafe { init_services(img, &syst) };
      info!("H2O UEFI loader for Oceanic OS .v3");

      outp::choose_mode(&syst, (1024, 768));
      outp::draw_logo(&syst);

      let h2o = file::load(&syst, "\\EFI\\Oceanic\\H2O.k");
      log::debug!(
            "Kernel file loaded at {:?}, ksize = {:?}",
            h2o.as_mut_ptr(),
            h2o.len()
      );
      let (entry, tls_size) = file::map_elf(&syst, unsafe { &mut *h2o });

      let mmap_size = mem::init_pf(&syst);
      let rsdp = mem::get_acpi_rsdp(&syst);

      let (mmap_paddr, mmap_buf) = mem::alloc(&syst)
            .alloc_into_slice(mmap_size * 2, mem::EFI_ID_OFFSET)
            .expect("Failed to allocate memory map buffer");
      syst.exit_boot_services(img, unsafe { &mut *mmap_buf })
            .expect_success("Failed to exit EFI boot services");

      mem::commit_mapping();

      let mut kmain = unsafe { core::mem::zeroed::<KMain>() };
      kmain.ptr = entry;
      unsafe { (kmain.func)(rsdp, mmap_paddr, tls_size.unwrap_or(0)) };

      // loop {
      //       unsafe { asm!("pause") }
      // }
}

#[inline]
fn round_up_p2(x: usize, u: usize) -> usize {
      (x.wrapping_sub(1) | (u - 1)).wrapping_add(1)
}
