#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(asm)]
#![feature(lang_items)]

extern crate alloc;

use log::*;
use uefi::prelude::*;
use uefi::proto::*;

static LOGO_FILE: &[u8] = include_bytes!("../../assets/Oceanic.500.bmp");

#[entry]
fn efi_main(_img: Handle, syst: SystemTable<Boot>) -> Status {
      uefi_services::init(&syst).expect_success("Failed to initialize EFI services");
      info!("hello world!");

      let gop = syst
            .boot_services()
            .locate_protocol::<console::gop::GraphicsOutput>()
            .expect_success("Failed to locate GOP");

      use console::gop::*;
      let res = (1024, 768);

      let modes = unsafe { &*gop.get() }.modes();
      for mode in modes {
            let mode = mode.unwrap();

            let minfo = mode.info();
            if minfo.resolution() == res {
                  unsafe { &mut *gop.get() }
                        .set_mode(&mode)
                        .expect_success("Unable to set graphics mode");
                  break;
            }
      }

      let bmp = tinybmp::Bmp::from_slice(LOGO_FILE).expect("Failed to load logo");
      let size = bmp.dimensions();
      let data = bmp.image_data();
      let data_as_blt = unsafe {
            core::slice::from_raw_parts(
                  data.as_ptr() as *const BltPixel,
                  data.len() * core::mem::size_of::<u8>() / core::mem::size_of::<BltPixel>(),
            )
      };
      let mut new_image = alloc::vec::Vec::with_capacity((size.0 + size.0 * size.1) as usize);
      new_image.extend_from_slice(data_as_blt);
      new_image.resize(new_image.len() + size.0 as usize, BltPixel::from(0));

      let dest = ((res.0 - size.0 as usize) / 2, (res.1 - size.1 as usize) / 2);

      unsafe { &mut *gop.get() }
            .blt(console::gop::BltOp::BufferToVideo {
                  buffer: &new_image,
                  src: BltRegion::Full,
                  dest,
                  dims: (size.0 as usize, size.1 as usize),
            })
            .expect_success("Failed to draw a logo");

      drop(new_image);

      loop {
            unsafe { asm!("pause") }
      }
}
