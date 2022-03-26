use sv_call::Feature;

use super::{
    hdl::{DefaultFeature, HandleMap},
    Tid,
};
use crate::{
    mem,
    sched::{
        wait::{Futex, FutexKey, FutexRef, Futexes},
        Arsc,
    },
};

#[derive(Debug)]
pub struct Space {
    mem: Arsc<mem::space::Space>,
    handles: HandleMap,
    futexes: Futexes,
}

unsafe impl Send for Space {}
unsafe impl Sync for Space {}

impl Space {
    pub fn new(ty: super::Type) -> sv_call::Result<Arsc<Self>> {
        let mem = mem::space::Space::try_new(ty)?;
        Arsc::try_new(Space {
            mem,
            handles: HandleMap::new(),
            futexes: Futexes::new(Default::default()),
        })
        .map_err(sv_call::Error::from)
    }

    pub fn new_current() -> sv_call::Result<Arsc<Self>> {
        Arsc::try_new(Space {
            mem: mem::space::with_current(Arsc::clone),
            handles: HandleMap::new(),
            futexes: Futexes::new(Default::default()),
        })
        .map_err(sv_call::Error::from)
    }

    #[inline]
    pub fn mem(&self) -> &Arsc<mem::space::Space> {
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

    pub fn child(&self, hdl: sv_call::Handle, need_feature: Feature) -> sv_call::Result<Tid> {
        super::PREEMPT.scope(|| {
            self.handles().get::<Tid>(hdl).and_then(|obj| {
                if obj.features().contains(need_feature) {
                    Ok(Tid::clone(&obj))
                } else {
                    Err(sv_call::Error::EPERM)
                }
            })
        })
    }
}

unsafe impl DefaultFeature for Arsc<Space> {
    fn default_features() -> Feature {
        Feature::SEND | Feature::SYNC | Feature::READ | Feature::WRITE
    }
}
