use core::{marker::PhantomData, mem, mem::MaybeUninit, num::NonZeroU64};

use solvent::{Result, SerdeReg};
pub use types::*;

use crate::{mem::space::PageFaultErrCode, sched::SCHED};

#[derive(Debug, Copy, Clone)]
pub struct UserPtr<T: Type, D> {
    data: *mut D,
    _marker: PhantomData<T>,
}

impl<T: Type, D> UserPtr<T, D> {
    pub fn new(data: *mut D) -> Self {
        UserPtr {
            data,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut D {
        self.data
    }

    pub fn check(&self) -> Result<()> {
        check_ptr(self.data.cast(), mem::size_of::<D>(), mem::align_of::<D>())
    }

    pub fn check_slice(&self, len: usize) -> Result<()> {
        check_ptr(
            self.data.cast(),
            mem::size_of::<T>() * len,
            mem::align_of::<D>(),
        )
    }
}

impl<D> UserPtr<In, D> {
    pub unsafe fn read(&self) -> Result<D> {
        self.check()?;

        let pf_resume = SCHED
            .with_current(|cur| cur.kstack_mut().pf_resume_mut())
            .ok_or(solvent::Error(solvent::ESRCH))?;

        let mut data = MaybeUninit::<D>::uninit();
        checked_copy(
            data.as_mut_ptr().cast(),
            self.data.cast(),
            pf_resume,
            mem::size_of::<D>(),
        )
        .into_result()?;

        Ok(data.assume_init())
    }

    pub unsafe fn read_slice(&self, out: *mut D, count: usize) -> Result<()> {
        self.check_slice(count)?;

        let pf_resume = SCHED
            .with_current(|cur| cur.kstack_mut().pf_resume_mut())
            .ok_or(solvent::Error(solvent::ESRCH))?;

        checked_copy(
            out.cast(),
            self.data.cast(),
            pf_resume,
            count * mem::size_of::<D>(),
        )
        .into_result()
    }
}

impl<D> UserPtr<Out, D> {
    pub unsafe fn write(&self, value: D) -> Result<()> {
        self.check()?;

        let pf_resume = SCHED
            .with_current(|cur| cur.kstack_mut().pf_resume_mut())
            .ok_or(solvent::Error(solvent::ESRCH))?;

        checked_copy(
            self.data.cast(),
            ((&value) as *const D).cast(),
            pf_resume,
            mem::size_of::<D>(),
        )
        .into_result()
    }

    pub unsafe fn write_slice(&self, value: &[D]) -> Result<()> {
        self.check_slice(value.len())?;

        let pf_resume = SCHED
            .with_current(|cur| cur.kstack_mut().pf_resume_mut())
            .ok_or(solvent::Error(solvent::ESRCH))?;

        checked_copy(
            self.data.cast(),
            value.as_ptr().cast(),
            pf_resume,
            value.len() * mem::size_of::<D>(),
        )
        .into_result()
    }
}

impl<D> UserPtr<InOut, D> {
    #[inline]
    pub fn r#in(&self) -> UserPtr<In, D> {
        UserPtr {
            data: self.data,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn out(&self) -> UserPtr<Out, D> {
        UserPtr {
            data: self.data,
            _marker: PhantomData,
        }
    }
}

impl<T: Type, D> SerdeReg for UserPtr<T, D> {
    #[inline]
    fn encode(self) -> usize {
        self.data as usize
    }

    #[inline]
    fn decode(val: usize) -> Self {
        UserPtr {
            data: val as *mut D,
            _marker: PhantomData,
        }
    }
}

fn check_ptr(ptr: *mut u8, size: usize, align: usize) -> Result<()> {
    let is_in_range =
        minfo::USER_BASE <= ptr as usize && (ptr as usize).saturating_add(size) <= minfo::USER_END;
    let is_aligned = (ptr as usize) & (align - 1) == 0;
    if is_in_range && is_aligned {
        Ok(())
    } else {
        Err(solvent::Error(solvent::EINVAL))
    }
}

mod types {
    pub enum In {}
    pub enum Out {}
    pub enum InOut {}

    pub trait Type {}
    impl Type for In {}
    impl Type for Out {}
    impl Type for InOut {}
}

#[repr(C)]
struct CheckedCopyRet {
    errc: PageFaultErrCode,
    addr_p1: u64,
}

impl CheckedCopyRet {
    fn into_result(self) -> Result<()> {
        if self.errc != PageFaultErrCode::empty() || self.addr_p1 != 0 {
            log::warn!(
                "Page fault at {:#x} during user pointer access",
                self.addr_p1 - 1
            );
            Err(solvent::Error(solvent::EPERM))
        } else {
            Ok(())
        }
    }
}

extern "C" {
    fn checked_copy(
        dst: *mut u8,
        src: *const u8,
        pf_resume: *mut Option<NonZeroU64>,
        count: usize,
    ) -> CheckedCopyRet;
}
