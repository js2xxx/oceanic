use core::{fmt, hash::Hash, marker::PhantomData, mem, mem::MaybeUninit, num::NonZeroU64};

use sv_call::{Result, SerdeReg};

pub use self::types::*;
use crate::{mem::space::PageFaultErrCode, sched::SCHED};

#[derive(Copy, Clone)]
pub struct UserPtr<T: Type, D> {
    data: *mut D,
    _marker: PhantomData<T>,
}

impl<T: Type, D> PartialEq for UserPtr<T, D> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl<T: Type, D> Hash for UserPtr<T, D> {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.data.hash(state);
    }
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

    /// # Errors
    ///
    /// Returns error if the pointer is unaligned or out of user address space.
    pub fn check(&self) -> Result<()> {
        check_ptr(self.data.cast(), mem::size_of::<D>(), mem::align_of::<D>())
    }

    /// # Errors
    ///
    /// Returns error if the pointer range is unaligned or out of user address
    /// space.
    pub fn check_slice(&self, len: usize) -> Result<()> {
        check_ptr(
            self.data.cast(),
            mem::size_of::<T>() * len,
            mem::align_of::<D>(),
        )
    }

    #[inline]
    pub fn cast<U>(self) -> UserPtr<T, U> {
        UserPtr {
            data: self.data.cast(),
            _marker: PhantomData,
        }
    }
}

impl<D> UserPtr<In, D> {
    /// # Errors
    ///
    /// Returns error if the pointer is invalid for reads or if the pointer is
    /// unaligned.
    ///
    /// # Safety
    ///
    /// Behavior is undefined if the pointer don't point to a properly
    /// initialized value of type `T`.
    pub unsafe fn read(&self) -> Result<D> {
        self.check()?;

        let pf_resume = SCHED.with_current(|cur| Ok(cur.kstack_mut().pf_resume_mut()))?;

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

    /// # Errors
    ///
    /// Returns error if the pointer is invalid for reads for `len` or if the
    /// pointer is unaligned.
    ///
    /// # Safety
    ///
    /// Behavior is undefined if the pointer don't point to a properly
    /// initialized array of type `T`.
    pub unsafe fn read_slice(&self, out: *mut D, count: usize) -> Result<()> {
        self.check_slice(count)?;

        let pf_resume = SCHED.with_current(|cur| Ok(cur.kstack_mut().pf_resume_mut()))?;

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
    /// # Errors
    ///
    /// Returns error if the pointer is invalid for writes or if the pointer is
    /// unaligned.
    pub fn write(&self, value: D) -> Result<()> {
        self.check()?;

        unsafe {
            let pf_resume = SCHED.with_current(|cur| Ok(cur.kstack_mut().pf_resume_mut()))?;

            checked_copy(
                self.data.cast(),
                ((&value) as *const D).cast(),
                pf_resume,
                mem::size_of::<D>(),
            )
            .into_result()
        }
    }

    /// # Errors
    ///
    /// Returns error if the pointer is invalid for writes or if the pointer is
    /// unaligned.
    pub fn write_slice(&self, value: &[D]) -> Result<()> {
        self.check_slice(value.len())?;

        unsafe {
            let pf_resume = SCHED.with_current(|cur| Ok(cur.kstack_mut().pf_resume_mut()))?;

            checked_copy(
                self.data.cast(),
                value.as_ptr().cast(),
                pf_resume,
                value.len() * mem::size_of::<D>(),
            )
            .into_result()
        }
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

impl<T: Type, D> fmt::Debug for UserPtr<T, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("UserPtr").field(&self.data).finish()
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
    // TODO: Decide whether to check the validity of pointers of empty slices.
    if !is_in_range && size > 0 {
        Err(sv_call::Error::EPERM)
    } else if !is_aligned {
        Err(sv_call::Error::EALIGN)
    } else {
        Ok(())
    }
}

mod types {
    #[derive(Copy, Clone)]
    pub enum In {}
    #[derive(Copy, Clone)]
    pub enum Out {}
    #[derive(Copy, Clone)]
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
                "Page fault at {:#x} during user pointer access, error code: {:?}",
                self.addr_p1 - 1,
                self.errc
            );
            Err(sv_call::Error::EPERM)
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
