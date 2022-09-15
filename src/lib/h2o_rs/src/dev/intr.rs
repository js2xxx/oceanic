use core::time::Duration;

pub use sv_call::res::IntrConfig;
use sv_call::{c_ty::Status, Syscall};

use super::GsiRes;
use crate::{error::Result, obj::Object, time::Instant};

#[repr(transparent)]
pub struct Interrupt(sv_call::Handle);
crate::impl_obj!(Interrupt);

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

    pub fn wait(&self, timeout: Duration) -> Result<Instant> {
        let mut ins = 0u128;
        unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_intr_wait(
                unsafe { self.raw() },
                crate::time::try_into_us(timeout)?,
                &mut ins as *mut _ as *mut _,
            )
            .into_res()?;
        }
        Ok(unsafe { Instant::from_raw(ins) })
    }

    pub fn pack_wait(&self, timeout: Duration) -> Result<PackIntrWait> {
        let mut ins = 0u128;
        let syscall = unsafe {
            // SAFETY: We don't move the ownership of the handle.
            sv_call::sv_pack_intr_wait(
                unsafe { self.raw() },
                crate::time::try_into_us(timeout)?,
                &mut ins as *mut _ as *mut _,
            )
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
    pub fn receive(&self, result: Status) -> Result<Instant> {
        result.into_res()?;
        Ok(unsafe { Instant::from_raw(self.ins) })
    }
}
