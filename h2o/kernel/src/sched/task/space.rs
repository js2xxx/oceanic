use alloc::sync::Arc;

use super::{hdl::HandleMap, Tid};
use crate::{
    mem,
    sched::wait::{Futex, FutexKey, FutexRef, Futexes},
};

#[derive(Debug)]
pub struct Space {
    mem: Arc<mem::space::Space>,
    handles: HandleMap,
    futexes: Futexes,
}

unsafe impl Send for Space {}
unsafe impl Sync for Space {}

impl Space {
    pub fn new(ty: super::Type) -> Arc<Self> {
        Arc::new(Space {
            mem: mem::space::Space::new(ty),
            handles: HandleMap::new(),
            futexes: Futexes::new(core::hash::BuildHasherDefault::default()),
        })
    }

    pub fn new_current() -> Arc<Self> {
        Arc::new(Space {
            mem: mem::space::with_current(Arc::clone),
            handles: HandleMap::new(),
            futexes: Futexes::new(core::hash::BuildHasherDefault::default()),
        })
    }

    #[inline]
    pub fn mem(&self) -> &Arc<mem::space::Space> {
        &self.mem
    }

    #[inline]
    pub fn handles(&self) -> &HandleMap {
        &self.handles
    }

    /// # Safety
    ///
    /// The function must be called when `PREEMPT` is disabled or locked.
    pub unsafe fn futex(&self, key: FutexKey) -> FutexRef {
        self.futexes.get_or_insert(key, Futex::new(key)).downgrade()
    }

    /// # Safety
    ///
    /// The function must be called when `PREEMPT` is disabled or locked.
    pub unsafe fn try_drop_futex(&self, key: FutexKey) {
        let _ = self.futexes.remove_if(&key, |futex| futex.is_empty());
    }

    pub fn child(&self, hdl: sv_call::Handle) -> sv_call::Result<Tid> {
        super::PREEMPT.scope(|| self.handles().get::<Tid>(hdl).map(|w| Tid::clone(w)))
    }
}
