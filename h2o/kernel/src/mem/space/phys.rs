mod contiguous;
mod extensible;

use paging::PAddr;
use sv_call::{Feature, Result};

use crate::{
    sched::task::hdl::DefaultFeature,
    syscall::{In, Out, UserPtr},
};

type Cont = self::contiguous::Phys;
type PinnedCont = self::contiguous::PinnedPhys;

#[derive(Debug, Clone, PartialEq)]
pub enum Phys {
    Contiguous(Cont),
}

#[derive(Debug)]
pub enum PinnedPhys {
    Contiguous(PinnedCont),
}

impl Phys {
    #[inline]
    pub fn new(base: PAddr, size: usize) -> Result<Self> {
        Ok(Phys::Contiguous(Cont::new(base, size)?))
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted or the size is zero.
    pub fn allocate(size: usize, zeroed: bool) -> Result<Self> {
        Ok(Phys::Contiguous(Cont::allocate(size, zeroed)?))
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Phys::Contiguous(cont) => cont.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        match self {
            Phys::Contiguous(cont) => cont.is_empty(),
        }
    }

    #[inline]
    pub fn pin(this: Self) -> Result<PinnedPhys> {
        match this {
            Phys::Contiguous(cont) => Ok(PinnedPhys::Contiguous(Cont::pin(cont))),
        }
    }

    #[inline]
    pub fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Self> {
        match self {
            Phys::Contiguous(cont) => cont.create_sub(offset, len, copy).map(Phys::Contiguous),
        }
    }

    #[inline]
    pub fn base(&self) -> PAddr {
        match self {
            Phys::Contiguous(cont) => cont.base(),
        }
    }

    #[inline]
    pub fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out, u8>) -> Result<usize> {
        match self {
            Phys::Contiguous(cont) => cont.read(offset, len, buffer),
        }
    }

    #[inline]
    pub fn write(&self, offset: usize, len: usize, buffer: UserPtr<In, u8>) -> Result<usize> {
        match self {
            Phys::Contiguous(cont) => cont.write(offset, len, buffer),
        }
    }
}

impl PinnedPhys {
    #[inline]
    pub fn map_iter(&self, offset: usize, len: usize) -> impl Iterator<Item = (PAddr, usize)> {
        match self {
            PinnedPhys::Contiguous(cont) => cont.map_iter(offset, len),
        }
    }
}

unsafe impl DefaultFeature for Phys {
    fn default_features() -> Feature {
        Feature::SEND | Feature::SYNC | Feature::READ | Feature::WRITE | Feature::EXECUTE
    }
}
