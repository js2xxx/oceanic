use alloc::sync::{Arc, Weak};
use core::{
    hash::BuildHasherDefault,
    num::NonZeroU64,
    ops::Deref,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering::*},
};

use archop::Azy;
use collection_ex::{CHashMap, FnvHasher};
use sv_call::Feature;

use super::{hdl::DefaultFeature, TaskInfo};
use crate::sched::PREEMPT;

pub const NR_TASKS: usize = 65536;

type BH = BuildHasherDefault<FnvHasher>;
static TI_MAP: Azy<CHashMap<u64, Arc<TaskInfo>, BH>> = Azy::new(Default::default);
static TASK_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub struct Tid {
    raw: NonZeroU64,
    ti: Arc<TaskInfo>,
}

#[derive(Debug, Clone)]
pub struct WeakTid {
    raw: Option<NonZeroU64>,
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
    pub fn raw(&self) -> u64 {
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

    pub fn raw(&self) -> Option<u64> {
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

fn next() -> Option<NonZeroU64> {
    static GEN: AtomicU64 = AtomicU64::new(1);
    let mut old = TASK_COUNT.load(Acquire);
    loop {
        if old >= NR_TASKS {
            return None;
        }
        match TASK_COUNT.compare_exchange(old, old + 1, SeqCst, SeqCst) {
            Ok(_) => return NonZeroU64::new(GEN.fetch_add(1, Relaxed)),
            Err(m) => old = m,
        }
    }
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

pub fn deallocate(tid: Tid) -> bool {
    let _flags = PREEMPT.lock();
    TI_MAP
        .remove(&tid.raw.get())
        .inspect(|_| {
            TASK_COUNT.fetch_sub(1, SeqCst);
        })
        .is_some()
}

#[inline]
pub fn init() {
    Azy::force(&TI_MAP);
}
