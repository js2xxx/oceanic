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

static mut LOGGER: MaybeUninit<Logger> = MaybeUninit::uninit();

unsafe fn init_log(syst: &SystemTable<Boot>, level: log::LevelFilter) {
      let stdout = syst.stdout();
      LOGGER.as_mut_ptr().write(Logger::new(stdout));
      log::set_logger(LOGGER.assume_init_ref()).expect("Failed to set logger");
      log::set_max_level(level);
}

#[entry]
fn efi_main(_img: Handle, syst: SystemTable<Boot>) -> Status {
      unsafe {
            uefi::alloc::init(&syst.boot_services());
            init_log(&syst, log::LevelFilter::max());
      }
      info!("H2O UEFI loader for Oceanic OS .v3");

      outp::choose_mode(&syst, (1024, 768));
      outp::draw_logo(&syst);

      let rsdp = mem::get_acpi_rsdp(&syst);
      mem::get_mapping(&syst);

      loop {
            unsafe { asm!("pause") }
      }
}
