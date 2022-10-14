use alloc::sync::{Arc, Weak};
use core::{hash::BuildHasherDefault, num::NonZeroU32, ops::Deref};

use archop::Azy;
use collection_ex::{CHashMap, FnvHasher, IdAllocator};
use sv_call::Feature;

use super::{hdl::DefaultFeature, TaskInfo};
use crate::sched::PREEMPT;

pub const NR_TASKS: usize = 65536;

type BH = BuildHasherDefault<FnvHasher>;
static TI_MAP: Azy<CHashMap<u32, Arc<TaskInfo>, BH>> = Azy::new(Default::default);
static TID_ALLOC: Azy<spin::Mutex<IdAllocator>> =
    Azy::new(|| spin::Mutex::new(IdAllocator::new(0..=(NR_TASKS as u64 - 1))));

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Tid {
    raw: NonZeroU32,
    ti: Arc<TaskInfo>,
}

#[derive(Debug, Clone)]
pub struct WeakTid {
    raw: Option<NonZeroU32>,
    ti: Weak<TaskInfo>,
}

impl Deref for Tid {
    type Target = TaskInfo;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.ti
    }
}

impl Tid {
    #[inline]
    pub fn raw(&self) -> u32 {
        self.raw.get()
    }

    #[inline]
    pub fn downgrade(&self) -> WeakTid {
        WeakTid {
            raw: Some(self.raw),
            ti: Arc::downgrade(&self.ti),
        }
    }
}

impl WeakTid {
    #[inline]
    pub fn new() -> Self {
        WeakTid {
            raw: None,
            ti: Weak::new(),
        }
    }

    pub fn raw(&self) -> Option<u32> {
        self.raw.map(|raw| raw.get())
    }

    pub fn upgrade(&self) -> Option<Tid> {
        let raw = self.raw?;
        let ti = self.ti.upgrade()?;
        Some(Tid { raw, ti })
    }
}

impl Default for WeakTid {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Tid {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw && Arc::ptr_eq(&self.ti, &other.ti)
    }
}

unsafe impl DefaultFeature for Tid {
    fn default_features() -> Feature {
        Feature::SEND | Feature::EXECUTE | Feature::WAIT
    }
}

fn next() -> Option<NonZeroU32> {
    let mut alloc = TID_ALLOC.lock();
    alloc
        .allocate()
        .and_then(|id| NonZeroU32::new((id + 1) as u32))
}

/// # Errors
///
/// Returns error if TID is exhausted.
pub fn allocate(ti: TaskInfo) -> sv_call::Result<Tid> {
    let _flags = PREEMPT.lock();
    match next() {
        Some(raw) => {
            let ti = Arc::try_new(ti)?;
            let old = TI_MAP.insert(raw.get(), ti.clone());
            debug_assert!(old.is_none());
            Ok(Tid { raw, ti })
        }
        None => Err(sv_call::ENOSPC),
    }
}

pub fn deallocate(tid: &Tid) -> bool {
    let _flags = PREEMPT.lock();
    TI_MAP.remove(&tid.raw.get()).map_or(false, |_| {
        TID_ALLOC.lock().deallocate(u64::from(tid.raw.get()));
        true
    })
}

#[inline]
pub fn init() {
    Azy::force(&TI_MAP);
    Azy::force(&TID_ALLOC);
}
