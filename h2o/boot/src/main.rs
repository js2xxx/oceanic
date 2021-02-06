#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(alloc_error_handler)]
#![feature(asm)]
#![feature(maybe_uninit_ref)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(panic_info_message)]

extern crate alloc;

mod file;
mod mem;
mod outp;
mod rxx;

use core::mem::{MaybeUninit, ManuallyDrop};
use log::*;
use uefi::logger::Logger;
use uefi::prelude::*;
use uefi::table::boot::{EventType, Tpl};

static mut LOGGER: MaybeUninit<Logger> = MaybeUninit::uninit();

unsafe fn init_log(syst: &SystemTable<Boot>, level: log::LevelFilter) {
      let stdout = syst.stdout();
      LOGGER.as_mut_ptr().write(Logger::new(stdout));
      log::set_logger(LOGGER.assume_init_ref()).expect("Failed to set logger");
      log::set_max_level(level);
}

unsafe fn init_services(img: Handle, syst: &SystemTable<Boot>) {
      fn exit_boot_services_callback(_: uefi::Event) {
            unsafe { LOGGER.assume_init_mut().disable() };
            uefi::alloc::exit_boot_services();
      }

      let bs = &syst.boot_services();

      uefi::alloc::init(bs);
      init_log(&syst, log::LevelFilter::max());

      bs.create_event(
            EventType::SIGNAL_EXIT_BOOT_SERVICES,
            Tpl::NOTIFY,
            Some(exit_boot_services_callback),
      )
      .expect_success("Failed to subscribe exit_boot_services callback");

      file::init(img, syst);
      mem::init();
}

#[entry]
fn efi_main(img: Handle, syst: SystemTable<Boot>) -> Status {
      unsafe { init_services(img, &syst) };
      info!("H2O UEFI loader for Oceanic OS .v3");

      outp::choose_mode(&syst, (1024, 768));
      outp::draw_logo(&syst);

      let h2o = ManuallyDrop::new(file::load("\\EFI\\Oceanic\\H2O.k"));
      let (entry, size) = file::map(&h2o);

      let rsdp = mem::get_acpi_rsdp(&syst);
      let mut buffer = alloc::vec![0; mem::PAGE_SIZE];
      mem::get_mmap(&syst, &mut buffer);

      loop {
            unsafe { asm!("pause") }
      }
}
