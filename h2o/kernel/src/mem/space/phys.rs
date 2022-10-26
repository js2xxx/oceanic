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

use self::extensible::*;

/// # Note
///
/// The task handle map doesn't support dynamic sized objects, and the vtable of
/// `PhysTrait` is very large (containing lots of function pointers), so we use
/// enum dispatch instead.
#[enum_dispatch(PhysTrait)]
#[derive(Debug, PartialEq)]
pub enum Phys {
    Cont,
    Static,
    Dynamic,
}

#[allow(clippy::len_without_is_empty)]
#[enum_dispatch]
pub trait PhysTrait {
    fn event(&self) -> Weak<dyn Event>;

    fn len(&self) -> usize;

    fn pin(&self, offset: usize, len: usize, write: bool) -> Result<Vec<(PAddr, usize)>>;

    fn create_sub(&self, offset: usize, len: usize, copy: bool) -> Result<Arc<Phys>>;

    fn base(&self) -> PAddr;

    fn resize(&self, new_len: usize, zeroed: bool) -> Result;

    fn read(&self, offset: usize, len: usize, buffer: UserPtr<Out>) -> Result<usize>;

    fn write(&self, offset: usize, len: usize, buffer: UserPtr<In>) -> Result<usize>;

    fn read_vectored(&self, offset: usize, bufs: &[(UserPtr<Out>, usize)]) -> Result<usize>;

    fn write_vectored(&self, offset: usize, bufs: &[(UserPtr<In>, usize)]) -> Result<usize>;
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
        let zeroed = options.contains(PhysOptions::ZEROED);
        if resizable {
            Phys::Dynamic(Dynamic::allocate(size, zeroed)?)
        } else {
            Phys::Static(Static::allocate(size, zeroed)?)
        }
    })?)
}
