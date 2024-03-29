mod contiguous;
mod extensible;

use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};

use enum_dispatch::enum_dispatch;
use paging::PAddr;
use sv_call::{mem::PhysOptions, Feature, Result, EPERM};

use crate::{
    sched::{task::hdl::DefaultFeature, Event},
    syscall::{In, Out, UserPtr},
};

type Cont = self::contiguous::Phys;

type Ext = self::extensible::Phys;

/// # Note
///
/// The task handle map doesn't support dynamic sized objects, and the vtable of
/// `PhysTrait` is very large (containing lots of function pointers), so we use
/// enum dispatch instead.
#[enum_dispatch(PhysTrait)]
#[derive(Debug, PartialEq)]
pub enum Phys {
    Cont,
    Ext,
}

#[allow(clippy::len_without_is_empty)]
#[enum_dispatch]
pub trait PhysTrait {
    fn event(&self) -> Weak<dyn Event>;

    fn len(&self) -> usize;

    fn pin(&self, offset: usize, len: usize, write: bool) -> Result<Vec<(PAddr, usize)>>;

    fn unpin(&self, offset: usize, len: usize);

    fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Arc<Phys>>;

    fn base(&self) -> PAddr;

    fn resize(&self, new_len: usize, zeroed: bool) -> Result;

    fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out>) -> Result<usize>;

    fn write(&self, offset: usize, len: usize, buffer: UserPtr<In>) -> Result<usize>;

    fn read_vectored(
        &self,
        mut offset: usize,
        bufs: &[(UserPtr<Out>, usize)],
    ) -> sv_call::Result<usize> {
        let mut read_len = 0;
        for (buffer, len) in bufs.iter().copied() {
            let actual = self.read(offset, len, buffer)?;
            read_len += actual;
            offset += actual;
            if actual < len {
                break;
            }
        }
        Ok(read_len)
    }

    fn write_vectored(&self, mut offset: usize, bufs: &[(UserPtr<In>, usize)]) -> Result<usize> {
        let mut written_len = 0;
        for (buffer, len) in bufs.iter().copied() {
            let actual = self.write(offset, len, buffer)?;
            written_len += actual;
            offset += actual;
            if actual < len {
                break;
            }
        }
        Ok(written_len)
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

#[inline]
pub fn new_phys(base: PAddr, size: usize) -> Result<Arc<Phys>> {
    Ok(Arc::try_new(Phys::from(Cont::new(base, size)?))?)
}

/// # Errors
///
/// Returns error if the heap memory is exhausted or the size is zero.
pub fn allocate_phys(size: usize, options: PhysOptions, contiguous: bool) -> Result<Arc<Phys>> {
    let resizable = options.contains(PhysOptions::RESIZABLE);
    Ok(Arc::try_new(if contiguous {
        if resizable {
            return Err(EPERM);
        }
        Phys::from(Cont::allocate(size, options.contains(PhysOptions::ZEROED))?)
    } else {
        Phys::from(Ext::new(size))
    })?)
}
