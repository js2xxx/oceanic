#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "alloc")]
use core::mem;

#[cfg(feature = "alloc")]
use sv_call::res::Msi;
use sv_call::{c_ty::Status, Syscall, ETIME, SV_INTERRUPT};

use super::IntrRes;
use crate::{error::Result, obj::Object, time::Instant};

#[repr(transparent)]
#[derive(Debug)]
pub struct Interrupt(sv_call::Handle);
crate::impl_obj!(Interrupt, SV_INTERRUPT);
crate::impl_obj!(@DROP, Interrupt);

#[derive(Debug, Copy, Clone, Default)]
pub struct IntrInfo {
    pub vec: u8,
    pub apic_id: u32,
}

impl Interrupt {
    pub fn allocate(res: &IntrRes) -> Result<(Interrupt, IntrInfo)> {
        let mut intr_info = IntrInfo::default();
        unsafe {
            // SAFETY: We don't move the ownership of the resource handle, and it represents
            // a valid interrupt resource.
            sv_call::sv_intr_new(unsafe { res.raw() }, &mut intr_info.vec as _, &mut intr_info.apic_id as _)
            .into_res()
            // SAFETY: The handle is freshly allocated.
            .map(|handle| (unsafe { Self::from_raw(handle) }, intr_info))
        }
    }

    #[cfg(feature = "alloc")]
    pub fn allocate_msi(res: &IntrRes, num: usize) -> Result<(Vec<Interrupt>, Msi)> {
        let mut intr = Vec::with_capacity(num);
        let mut msi = Msi::default();

        unsafe {
            // SAFETY: We don't move the ownership of the resource handle, and it represents
            // a valid interrupt resource.
            sv_call::sv_intr_msi(unsafe { res.raw() }, num, intr.as_mut_ptr(), &mut msi as _)
        }
        .into_res()?;

        // SAFETY: The interrupts are filled in, and #[repr(transparent)] so we can
        // transmute it directly.
        unsafe {
            intr.set_len(num);
            Ok((mem::transmute(intr), msi))
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
