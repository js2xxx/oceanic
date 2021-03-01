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
use uefi::logger::Logger;
use uefi::prelude::*;
use uefi::table::boot::{EventType, Tpl};

static mut LOGGER: MaybeUninit<Logger> = MaybeUninit::uninit();

unsafe fn call_kmain(
      entry: *mut u8,
      rsdp: *const core::ffi::c_void,
      efi_mmap_paddr: paging::PAddr,
      efi_mmap_len: usize,
      efi_mmap_unit: usize,
      tls_size: usize,
) {
      asm!(
            "call {}",
            in(reg) entry,
            in("rdi") rsdp,
            in("rsi") *efi_mmap_paddr,
            in("rdx") efi_mmap_len,
            in("rcx") efi_mmap_unit,
            in("r8") tls_size,
      );
}

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
            log::debug!("Reaching end of H2O boot loader");
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

      // Load and map the kernel file. Note: in the future, the file(s) needed to be loaded and
      // mapped may get more and more.
      let h2o = file::load(&syst, "\\EFI\\Oceanic\\H2O.k");
      log::debug!(
            "Kernel file loaded at {:?}, ksize = {:?}",
            h2o.as_mut_ptr(),
            h2o.len()
      );
      let (entry, tls_size) = file::map_elf(&syst, unsafe { &mut *h2o });

      // Prepare the data needed for H2O.
      let (mmap_unit, mmap_size_approx) = mem::init_pf(&syst);
      let rsdp = mem::get_acpi_rsdp(&syst);

      // Get the EFI memory map to be parsed in the kernel. So far we cannot parse it in the loader
      // because if we make dynamic space after get the map, the map will be updated immediately
      // and the key will become invalid.
      let (mmap_paddr, mmap_buf) = mem::alloc(&syst)
            .alloc_into_slice(mmap_size_approx * 2, mem::EFI_ID_OFFSET)
            .expect("Failed to allocate memory map buffer");
      let (_rt, mmap) = syst
            .exit_boot_services(img, unsafe { &mut *mmap_buf })
            .expect_success("Failed to exit EFI boot services");
      let mmap_len = mmap.len();
      // mem::config_efi_runtime(&rt, mmap);

      mem::commit_mapping();

      unsafe {
            call_kmain(
                  entry,
                  rsdp,
                  mmap_paddr,
                  mmap_len,
                  mmap_unit,
                  tls_size.unwrap_or(0),
            )
      };

      // This dummy code is for debugging.
      loop {
            core::hint::spin_loop();
      }
}
