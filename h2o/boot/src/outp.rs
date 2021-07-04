use alloc::vec::Vec;
use core::ptr::NonNull;
use uefi::prelude::*;
use uefi::proto::console::gop::*;

static LOGO_FILE: &[u8] = include_bytes!("../../assets/Oceanic.500.bmp");
static mut RESOLUTION: Option<(usize, usize)> = None;

unsafe fn gop<'a>(syst: &SystemTable<Boot>) -> NonNull<GraphicsOutput<'a>> {
      NonNull::new_unchecked(
            syst.boot_services()
                  .locate_protocol::<GraphicsOutput>()
                  .expect_success("Failed to locate graphics output protocol")
                  .get(),
      )
}

pub fn choose_mode(syst: &SystemTable<Boot>, preferred_res: (usize, usize)) -> (usize, usize) {
      log::trace!(
            "outp::choose_mode: syst = {:?}, preferred_res = {:?}",
            syst as *const _,
            preferred_res
      );

      let mut gop = unsafe { self::gop(syst) };
      let mode = {
            let mut selected = None;
            let modes = unsafe { gop.as_ref() }.modes();
            for mode in modes {
                  let mode = mode.unwrap();

                  let minfo = mode.info();
                  if minfo.resolution() == preferred_res && minfo.pixel_format() == PixelFormat::Bgr
                  {
                        selected = Some(mode);
                  }
            }
            selected.expect("Failed to find a proper mode")
      };

      unsafe {
            RESOLUTION = Some(mode.info().resolution());
            gop.as_mut()
                  .set_mode(&mode)
                  .expect_success("Failed to set mode");

            RESOLUTION.unwrap()
      }
}

fn get_logo_data() -> (Vec<BltPixel>, (usize, usize)) {
      log::trace!("outp::get_logo_data");

      let bmp = tinybmp::RawBmp::from_slice(LOGO_FILE).expect("Failed to load logo");

      let (w, h) = {
            let size = bmp.header().image_size;
            (size.width, size.height)
      };

      let blt_data = unsafe {
            let data = bmp.image_data();
            core::slice::from_raw_parts(
                  data.as_ptr().cast(),
                  data.len() * core::mem::size_of::<u8>() / core::mem::size_of::<BltPixel>(),
            )
      };

      let mut blt_buffer =
            alloc::vec::Vec::with_capacity((w + w * h) as usize);
      blt_buffer.extend_from_slice(blt_data);
      blt_buffer.resize(blt_buffer.len() + w as usize, BltPixel::from(0));

      (blt_buffer, (w as usize, h as usize))
}

pub fn draw_logo(syst: &SystemTable<Boot>) {
      log::trace!("outp::draw_logo: syst = {:?}", syst as *const _);

      let (blt_buffer, dims) = get_logo_data();

      let res = unsafe { RESOLUTION.expect("Unset resolution (should it be initialized?)") };
      let dest = ((res.0 - dims.0 as usize) / 2, (res.1 - dims.1 as usize) / 2);

      unsafe { self::gop(syst).as_mut() }
            .blt(BltOp::BufferToVideo {
                  buffer: &blt_buffer,
                  src: BltRegion::Full,
                  dest,
                  dims,
            })
            .expect_success("Failed to draw a logo");
}
