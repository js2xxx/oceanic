use alloc::sync::Arc;
use core::{hash::BuildHasherDefault, num::NonZeroU32, ops::Deref};

use collection_ex::{CHashMap, FnvHasher, IdAllocator};
use solvent::Handle;
use spin::Lazy;

use super::TaskInfo;
use crate::sched::PREEMPT;

pub const NR_TASKS: usize = 65536;

type BH = BuildHasherDefault<FnvHasher>;
static TI_MAP: Lazy<CHashMap<u32, Arc<TaskInfo>, BH>> = Lazy::new(|| CHashMap::new(BH::default()));
static TID_ALLOC: Lazy<spin::Mutex<IdAllocator>> =
    Lazy::new(|| spin::Mutex::new(IdAllocator::new(0..=(NR_TASKS as u64 - 1))));

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Tid {
    raw: NonZeroU32,
    ti: Arc<TaskInfo>,
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

    pub fn child(&self, hdl: Handle) -> Option<Tid> {
        super::PREEMPT.scope(|| self.handles().get::<Tid>(hdl).map(|w| Tid::clone(&w)))
    }

    pub fn drop_child(&self, hdl: Handle) -> Option<Tid> {
        super::PREEMPT.scope(|| self.handles().remove::<Tid>(hdl))
    }
}

impl PartialEq for Tid {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw && Arc::ptr_eq(&self.ti, &other.ti)
    }
}

fn next() -> Option<NonZeroU32> {
    let mut alloc = TID_ALLOC.lock();
    alloc
        .allocate()
        .map(|id| NonZeroU32::new((id + 1) as u32).unwrap())
}

/// # Errors
///
/// Returns error if TID is exhausted.
pub fn allocate(ti: TaskInfo) -> Result<Tid, TaskInfo> {
    allocate_or(ti, |ti| ti)
}

/// # Errors
///
/// Returns error if TID is exhausted.
pub fn allocate_or<F, R>(ti: TaskInfo, or_else: F) -> Result<Tid, R>
where
    F: FnOnce(TaskInfo) -> R,
{
    let _flags = PREEMPT.lock();
    match next() {
        Some(raw) => {
            let ti = Arc::new(ti);
            let old = TI_MAP.insert(raw.get(), ti.clone());
            debug_assert!(old.is_none());
            Ok(Tid { raw, ti })
        }
        None => Err(or_else(ti)),
    }
}

pub fn deallocate(tid: &Tid) -> bool {
    let _flags = PREEMPT.lock();
    TI_MAP.remove(&tid.raw.get()).map_or(false, |_| {
        TID_ALLOC.lock().deallocate(u64::from(tid.raw.get()));
        true
    })
}

pub fn has_ti(tid: &Tid) -> bool {
    let _flags = PREEMPT.lock();
    TI_MAP.contains_key(&tid.raw.get())
}
