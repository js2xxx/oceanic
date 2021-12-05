use core::{marker::PhantomData, mem, ptr::NonNull};

use solvent::{Result, SerdeReg};
pub use types::*;

#[derive(Debug, Copy, Clone)]
pub struct UserPtr<T: Type, D> {
    data: *mut D,
    _marker: PhantomData<T>,
}

impl<T: Type, D> UserPtr<T, D> {
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

    pub fn null_or<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(Option<NonNull<D>>) -> Result<R>,
    {
        if self.data.is_null() {
            f(None)
        } else {
            self.check()?;
            f(NonNull::new(self.data))
        }
    }

    pub fn null_or_slice<F, R>(&self, len: usize, f: F) -> Result<R>
    where
        F: FnOnce(Option<NonNull<[D]>>) -> Result<R>,
    {
        if self.data.is_null() {
            f(None)
        } else {
            self.check_slice(len)?;
            f(NonNull::new(self.data).map(|ptr| NonNull::slice_from_raw_parts(ptr, len)))
        }
    }
}

impl<D> UserPtr<In, D> {
    pub unsafe fn read(&self) -> Result<D> {
        self.check()?;
        Ok(self.data.read_volatile())
    }

    pub unsafe fn read_slice(&self, out: *mut D, count: usize) -> Result<()> {
        self.check_slice(count)?;
        Ok(out.copy_from_nonoverlapping(self.data, count))
    }
}

impl<D> UserPtr<Out, D> {
    pub unsafe fn write(&self, value: D) -> Result<()> {
        self.check()?;
        Ok(self.data.write_volatile(value))
    }

    pub unsafe fn write_slice(&self, value: &[D]) -> Result<()> {
        self.check_slice(value.len())?;
        Ok(self
            .data
            .copy_from_nonoverlapping(value.as_ptr(), value.len()))
    }
}

impl<D> UserPtr<InOut, D> {
    pub fn r#in(&self) -> UserPtr<In, D> {
        UserPtr {
            data: self.data,
            _marker: PhantomData,
        }
    }

    pub fn out(&self) -> UserPtr<Out, D> {
        UserPtr {
            data: self.data,
            _marker: PhantomData,
        }
    }
}

impl<T: Type, D> SerdeReg for UserPtr<T, D> {
    fn encode(self) -> usize {
        self.data as usize
    }

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
