pub use sv_call::res::IntrConfig;
use sv_call::{c_ty::Status, Syscall, ETIME, SV_INTERRUPT};

use super::GsiRes;
use crate::{error::Result, obj::Object, time::Instant};

#[repr(transparent)]
#[derive(Debug)]
pub struct Interrupt(sv_call::Handle);
crate::impl_obj!(Interrupt, SV_INTERRUPT);

impl Interrupt {
    pub fn acquire(res: &GsiRes, gsi: u32, config: IntrConfig) -> Result<Interrupt> {
        unsafe {
            // SAFETY: We don't move the ownership of the resource handle, and it represents
            // a valid GSI resource.
            sv_call::sv_intr_new(unsafe { res.raw() }, gsi, config)
            .into_res()
            // SAFETY: The handle is freshly allocated.
            .map(|handle| unsafe { Self::from_raw(handle) })
        }
    }

    pub fn last_time(&self) -> Result<Instant> {
        let mut ins = 0u128;
        unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_intr_query(unsafe { self.raw() }, &mut ins as *mut _ as *mut _)
                .into_res()?;
        }
        Ok(unsafe { Instant::from_raw(ins) })
    }

    pub fn pack_query(&self) -> Result<PackIntrWait> {
        let mut ins = 0u128;
        let syscall = unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_pack_intr_query(unsafe { self.raw() }, &mut ins as *mut _ as *mut _)
        };
        Ok(PackIntrWait { ins, syscall })
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        unsafe {
            sv_call::sv_intr_drop(unsafe { self.raw() })
                .into_res()
                .expect("Failed to drop an interrupt");
        }
    }
}

pub struct PackIntrWait {
    pub ins: u128,
    pub syscall: Syscall,
}

impl PackIntrWait {
    #[inline]
    pub fn receive(&self, res: Status, canceled: bool) -> Result<Instant> {
        res.into_res().and((!canceled).then_some(()).ok_or(ETIME))?;
        Ok(unsafe { Instant::from_raw(self.ins) })
    }
}
