#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(asm)]
#![feature(lang_items)]

use log::*;
use uefi::prelude::*;

#[entry]
fn efi_main(_img: Handle, syst: SystemTable<Boot>) -> Status {
      uefi_services::init(&syst).expect_success("Failed to initialize EFI services");
      info!("hello world!");
      loop {
            unsafe { asm!("pause") }
      }
}
