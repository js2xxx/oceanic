#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(asm)]
#![feature(lang_items)]

use log::*;
use uefi::prelude::*;
use uefi::proto::*;

#[entry]
fn efi_main(_img: Handle, syst: SystemTable<Boot>) -> Status {
      uefi_services::init(&syst).expect_success("Failed to initialize EFI services");
      info!("hello world!");

      let gop = syst
            .boot_services()
            .locate_protocol::<console::gop::GraphicsOutput>()
            .expect_success("Failed to locate GOP");

      let modes = unsafe { &*gop.get() }.modes();
      for mode in modes {
            let mode = mode.unwrap();

            let minfo = mode.info();
            let res = minfo.resolution();
            let pix = minfo.pixel_format();
            let stride = minfo.stride();

            info!("Mode: {}x{}: {:?} ({})", res.0, res.1, pix, stride);
      }
      loop {
            unsafe { asm!("pause") }
      }
}
