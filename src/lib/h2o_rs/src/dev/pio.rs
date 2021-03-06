use core::ops::Range;

use super::PioRes;
use crate::{error::Result, obj::Object};

pub struct PortIo<'a> {
    range: Range<u16>,
    res: &'a PioRes,
}

impl<'a> PortIo<'a> {
    pub fn acquire(res: &PioRes, range: Range<u16>) -> Result<PortIo> {
        unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_pio_acq(unsafe { res.raw() }, range.start, range.end - range.start)
                .into_res()?;
        }
        Ok(PortIo { range, res })
    }
}

impl<'a> Drop for PortIo<'a> {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_pio_rel(
                unsafe { self.res.raw() },
                self.range.start,
                self.range.end - self.range.start,
            )
            .into_res()
            .expect("Failed to release port I/O");
        }
    }
}
