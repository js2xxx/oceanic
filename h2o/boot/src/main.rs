#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(asm)]
#![feature(lang_items)]

extern crate alloc;

mod display;

use log::*;
use uefi::prelude::*;

#[entry]
fn efi_main(_img: Handle, syst: SystemTable<Boot>) -> Status {
      uefi_services::init(&syst).expect_success("Failed to initialize EFI services");
      info!("H2O UEFI loader for Oceanic OS .v3");

      display::choose_mode((1024, 768));
      display::draw_logo();

      loop {
            unsafe { asm!("pause") }
      }
}
