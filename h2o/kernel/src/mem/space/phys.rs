mod contiguous;
mod extensible;

use alloc::sync::Weak;

use paging::PAddr;
use sv_call::{Feature, Result, EPERM};

use crate::{
    sched::{task::hdl::DefaultFeature, BasicEvent, Event},
    syscall::{In, Out, UserPtr},
};

type Cont = self::contiguous::Phys;
type PinnedCont = self::contiguous::PinnedPhys;

type Ext = self::extensible::Phys;
type PinnedExt = self::extensible::PinnedPhys;

#[derive(Debug, Clone, PartialEq)]
pub enum Phys {
    Contiguous(Cont),
    Extensible(Ext),
}

#[derive(Debug)]
pub enum PinnedPhys {
    Contiguous(PinnedCont),
    Extensible(PinnedExt),
}

impl Phys {
    #[inline]
    pub fn new(base: PAddr, size: usize) -> Result<Self> {
        Ok(Phys::Contiguous(Cont::new(base, size)?))
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted or the size is zero.
    pub fn allocate(size: usize, zeroed: bool, contiguous: bool) -> Result<Self> {
        Ok(if contiguous {
            Phys::Contiguous(Cont::allocate(size, zeroed)?)
        } else {
            Phys::Extensible(Ext::allocate(size, zeroed)?)
        })
    }

    pub fn event(&self) -> Weak<dyn Event> {
        match self {
            Phys::Contiguous(_) => Weak::<BasicEvent>::new() as _,
            Phys::Extensible(ext) => ext.event(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Phys::Contiguous(cont) => cont.len(),
            Phys::Extensible(ext) => ext.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        match self {
            Phys::Contiguous(cont) => cont.is_empty(),
            Phys::Extensible(ext) => ext.is_empty(),
        }
    }

    #[inline]
    pub fn pin(this: Self) -> Result<PinnedPhys> {
        match this {
            Phys::Contiguous(cont) => Ok(PinnedPhys::Contiguous(Cont::pin(cont))),
            Phys::Extensible(ext) => Ok(PinnedPhys::Extensible(Ext::pin(ext)?)),
        }
    }

    #[inline]
    pub fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Self> {
        match self {
            Phys::Contiguous(cont) => cont.create_sub(offset, len, copy).map(Phys::Contiguous),
            Phys::Extensible(ext) => ext.create_sub(offset, len, copy).map(Phys::Extensible),
        }
    }

    #[inline]
    pub fn base(&self) -> PAddr {
        match self {
            Phys::Contiguous(cont) => cont.base(),
            Phys::Extensible(_) => unimplemented!("Extensible phys have multiple bases"),
        }
    }

    #[inline]
    pub fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out, u8>) -> Result<usize> {
        match self {
            Phys::Contiguous(cont) => cont.read(offset, len, buffer),
            Phys::Extensible(ext) => ext.read(offset, len, buffer),
        }
    }

    #[inline]
    pub fn write(&self, offset: usize, len: usize, buffer: UserPtr<In, u8>) -> Result<usize> {
        match self {
            Phys::Contiguous(cont) => cont.write(offset, len, buffer),
            Phys::Extensible(ext) => ext.write(offset, len, buffer),
        }
    }

    #[inline]
    pub fn resize(&self, new_len: usize, zeroed: bool) -> Result {
        match self {
            Phys::Contiguous(_) => Err(EPERM),
            Phys::Extensible(ext) => ext.resize(new_len, zeroed),
        }
    }
}

impl PinnedPhys {
    #[inline]
    pub fn map_iter(&self, offset: usize, len: usize) -> impl Iterator<Item = (PAddr, usize)> + '_ {
        enum Either<A, B> {
            A(A),
            B(B),
        }
        impl<A, B, T> Iterator for Either<A, B>
        where
            A: Iterator<Item = T>,
            B: Iterator<Item = T>,
        {
            type Item = T;
            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    Either::A(a) => a.next(),
                    Either::B(b) => b.next(),
                }
            }
        }

        match self {
            PinnedPhys::Contiguous(cont) => Either::A(cont.map_iter(offset, len)),
            PinnedPhys::Extensible(ext) => Either::B(ext.map_iter(offset, len)),
        }
    }
}

unsafe impl DefaultFeature for Phys {
    fn default_features() -> Feature {
        Feature::SEND
            | Feature::SYNC
            | Feature::READ
            | Feature::WRITE
            | Feature::EXECUTE
            | Feature::WAIT
    }
}
