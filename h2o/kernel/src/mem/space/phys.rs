mod contiguous;
mod extensible;

use alloc::sync::Weak;

use paging::PAddr;
use sv_call::{mem::PhysOptions, Feature, Result, EPERM};

use crate::{
    sched::{task::hdl::DefaultFeature, BasicEvent, Event},
    syscall::{In, Out, UserPtr},
};

type Cont = self::contiguous::Phys;
type PinnedCont = self::contiguous::PinnedPhys;

use self::extensible::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Phys {
    Contiguous(Cont),
    Static(Static),
    Dynamic(Dynamic),
}

#[derive(Debug)]
pub enum PinnedPhys {
    Contiguous(PinnedCont),
    Static(PinnedStatic),
    Dynamic(PinnedDynamic),
}

impl Phys {
    #[inline]
    pub fn new(base: PAddr, size: usize) -> Result<Self> {
        Ok(Phys::Contiguous(Cont::new(base, size)?))
    }

    /// # Errors
    ///
    /// Returns error if the heap memory is exhausted or the size is zero.
    pub fn allocate(size: usize, options: PhysOptions, contiguous: bool) -> Result<Self> {
        let resizable = options.contains(PhysOptions::RESIZABLE);
        Ok(if contiguous {
            if resizable {
                return Err(EPERM);
            }
            Phys::Contiguous(Cont::allocate(size, options.contains(PhysOptions::ZEROED))?)
        } else {
            let zeroed = options.contains(PhysOptions::ZEROED);
            if resizable {
                Phys::Dynamic(Dynamic::allocate(size, zeroed)?)
            } else {
                Phys::Static(Static::allocate(size, zeroed)?)
            }
        })
    }

    pub fn event(&self) -> Weak<dyn Event> {
        match self {
            Phys::Dynamic(d) => d.event(),
            _ => Weak::<BasicEvent>::new() as _,
        }
    }

    #[inline]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            Phys::Contiguous(cont) => cont.len(),
            Phys::Static(s) => s.len(),
            Phys::Dynamic(d) => d.len(),
        }
    }

    #[inline]
    pub fn pin(this: Self) -> Result<PinnedPhys> {
        match this {
            Phys::Contiguous(cont) => Ok(PinnedPhys::Contiguous(Cont::pin(cont))),
            Phys::Static(ext) => Ok(PinnedPhys::Static(Static::pin(ext))),
            Phys::Dynamic(ext) => Ok(PinnedPhys::Dynamic(Dynamic::pin(ext)?)),
        }
    }

    #[inline]
    pub fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Self> {
        match self {
            Phys::Contiguous(cont) => cont.create_sub(offset, len, copy).map(Phys::Contiguous),
            Phys::Static(ext) => ext.create_sub(offset, len, copy).map(Phys::Static),
            Phys::Dynamic(_) => Err(EPERM),
        }
    }

    #[inline]
    pub fn base(&self) -> PAddr {
        match self {
            Phys::Contiguous(cont) => cont.base(),
            _ => unimplemented!("Extensible phys have multiple bases"),
        }
    }

    #[inline]
    pub fn resize(&self, new_len: usize, zeroed: bool) -> Result {
        match self {
            Phys::Dynamic(d) => d.resize(new_len, zeroed),
            _ => Err(EPERM),
        }
    }

    #[inline]
    pub fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out>) -> Result<usize> {
        match self {
            Phys::Contiguous(cont) => cont.read(offset, len, buffer),
            Phys::Static(s) => s.read(offset, len, buffer),
            Phys::Dynamic(d) => d.read(offset, len, buffer),
        }
    }

    #[inline]
    pub fn write(&self, offset: usize, len: usize, buffer: UserPtr<In>) -> Result<usize> {
        match self {
            Phys::Contiguous(cont) => cont.write(offset, len, buffer),
            Phys::Static(s) => s.write(offset, len, buffer),
            Phys::Dynamic(d) => d.write(offset, len, buffer),
        }
    }

    #[inline]
    pub fn read_vectored(&self, offset: usize, bufs: &[(UserPtr<Out>, usize)]) -> Result<usize> {
        match self {
            Phys::Contiguous(cont) => cont.read_vectored(offset, bufs),
            Phys::Static(s) => s.read_vectored(offset, bufs),
            Phys::Dynamic(d) => d.read_vectored(offset, bufs),
        }
    }

    #[inline]
    pub fn write_vectored(&self, offset: usize, bufs: &[(UserPtr<In>, usize)]) -> Result<usize> {
        match self {
            Phys::Contiguous(cont) => cont.write_vectored(offset, bufs),
            Phys::Static(s) => s.write_vectored(offset, bufs),
            Phys::Dynamic(d) => d.write_vectored(offset, bufs),
        }
    }
}

impl PinnedPhys {
    #[inline]
    pub fn map_iter(&self, offset: usize, len: usize) -> impl Iterator<Item = (PAddr, usize)> + '_ {
        enum OneOf<A, B, C> {
            A(A),
            B(B),
            C(C),
        }
        impl<A, B, C, T> Iterator for OneOf<A, B, C>
        where
            A: Iterator<Item = T>,
            B: Iterator<Item = T>,
            C: Iterator<Item = T>,
        {
            type Item = T;
            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    OneOf::A(a) => a.next(),
                    OneOf::B(b) => b.next(),
                    OneOf::C(c) => c.next(),
                }
            }
        }

        match self {
            PinnedPhys::Contiguous(cont) => OneOf::A(cont.map_iter(offset, len)),
            PinnedPhys::Static(s) => OneOf::B(s.map_iter(offset, len)),
            PinnedPhys::Dynamic(d) => OneOf::C(d.map_iter(offset, len)),
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
