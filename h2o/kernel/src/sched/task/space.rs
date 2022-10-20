use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering::*};

use sv_call::Feature;

use super::{
    hdl::{DefaultFeature, HandleMap},
    Tid,
};
use crate::{
    mem,
    sched::wait::{Futex, FutexKey, FutexRef, Futexes},
};

#[derive(Debug)]
pub struct Space {
    mem: Arc<mem::space::Space>,
    handles: HandleMap,
    futexes: Futexes,
    main: AtomicU32,
}

unsafe impl Send for Space {}
unsafe impl Sync for Space {}

impl Space {
    pub fn new(ty: super::Type) -> sv_call::Result<Arc<Self>> {
        let mem = mem::space::Space::try_new(ty)?;
        Ok(Arc::try_new(Space {
            mem,
            handles: HandleMap::new(),
            futexes: Default::default(),
            main: AtomicU32::new(0),
        })?)
    }

    pub fn new_current() -> Arc<Self> {
        Arc::new(Space {
            mem: mem::space::with_current(Arc::clone),
            handles: HandleMap::new(),
            futexes: Default::default(),
            main: AtomicU32::new(0),
        })
    }

    #[inline]
    pub fn mem(&self) -> &Arc<mem::space::Space> {
        &self.mem
    }

    #[inline]
    pub fn set_main(&self, tid: &Tid) {
        let _ = self.main.compare_exchange(0, tid.raw(), AcqRel, Acquire);
    }

    #[inline]
    pub fn try_stop(&self, tid: &Tid) {
        let _ = self.main.compare_exchange(tid.raw(), 0, AcqRel, Acquire);
    }

    #[inline]
    pub fn has_to_stop(&self) -> bool {
        self.main.load(Acquire) == 0
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

    pub fn child(&self, hdl: sv_call::Handle, need_feature: Feature) -> sv_call::Result<Tid> {
        super::PREEMPT.scope(|| {
            self.handles().get::<Tid>(hdl).and_then(|obj| {
                if obj.features().contains(need_feature) {
                    Ok(Tid::clone(&obj))
                } else {
                    Err(sv_call::EPERM)
                }
            })
        })
    }
}

unsafe impl DefaultFeature for Space {
    fn default_features() -> Feature {
        Feature::READ | Feature::WRITE
    }
}
