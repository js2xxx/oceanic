#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(alloc_error_handler)]
#![feature(asm)]
#![feature(maybe_uninit_ref)]
#![feature(panic_info_message)]

extern crate alloc;

mod mem;
mod outp;
mod rxx;

use core::mem::MaybeUninit;
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

unsafe fn init_services(syst: &SystemTable<Boot>) {
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
}

#[entry]
fn efi_main(_img: Handle, syst: SystemTable<Boot>) -> Status {
      unsafe { init_services(&syst) };
      info!("H2O UEFI loader for Oceanic OS .v3");

      outp::choose_mode(&syst, (1024, 768));
      outp::draw_logo(&syst);

      let rsdp = mem::get_acpi_rsdp(&syst);
      mem::get_mapping(&syst);

      loop {
            unsafe { asm!("pause") }
      }
}
