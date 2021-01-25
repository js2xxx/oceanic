use alloc::vec::Vec;
use core::ptr::NonNull;
use uefi::prelude::*;
use uefi::proto::console::gop::*;

static LOGO_FILE: &[u8] = include_bytes!("../../assets/Oceanic.500.bmp");
static mut RESOLUTION: Option<(usize, usize)> = None;

unsafe fn gop() -> NonNull<GraphicsOutput<'static>> {
      let syst = uefi_services::system_table();
      NonNull::new_unchecked(
            syst.as_ref()
                  .boot_services()
                  .locate_protocol::<GraphicsOutput>()
                  .expect_success("Failed to locate graphics output protocol")
                  .get(),
      )
}

pub fn choose_mode(preferred_res: (usize, usize)) -> (usize, usize) {
      let mut gop = unsafe { self::gop() };
      let mode = {
            let modes = unsafe { gop.as_ref() }.modes();
            let mut selected = None;
            for mode in modes {
                  let mode = mode.unwrap();

                  let minfo = mode.info();
                  if minfo.resolution() == preferred_res && minfo.pixel_format() == PixelFormat::BGR
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
      let bmp = tinybmp::Bmp::from_slice(LOGO_FILE).expect("Failed to load logo");

      let size = bmp.dimensions();

      let blt_data = unsafe {
            let data = bmp.image_data();
            core::slice::from_raw_parts(
                  data.as_ptr() as *const BltPixel,
                  data.len() * core::mem::size_of::<u8>() / core::mem::size_of::<BltPixel>(),
            )
      };

      let mut blt_buffer = alloc::vec::Vec::with_capacity((size.0 + size.0 * size.1) as usize);
      blt_buffer.extend_from_slice(blt_data);
      blt_buffer.resize(blt_buffer.len() + size.0 as usize, BltPixel::from(0));

      (blt_buffer, (size.0 as usize, size.1 as usize))
}

pub fn draw_logo() {
      let mut gop = unsafe { self::gop() };

      let (blt_buffer, dims) = get_logo_data();

      let res = unsafe { RESOLUTION.expect("Unset resolution (should it be initialized?)") };
      let dest = ((res.0 - dims.0 as usize) / 2, (res.1 - dims.1 as usize) / 2);

      unsafe { gop.as_mut() }
            .blt(BltOp::BufferToVideo {
                  buffer: &blt_buffer,
                  src: BltRegion::Full,
                  dest,
                  dims,
            })
            .expect_success("Failed to draw a logo");
}
