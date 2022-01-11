use alloc::sync::Arc;
use core::{hash::BuildHasherDefault, ops::Deref};

use collection_ex::{CHashMap, FnvHasher, IdAllocator};
use solvent::Handle;
use spin::Lazy;

use super::{Child, TaskInfo};
use crate::sched::PREEMPT;

pub const NR_TASKS: usize = 65536;

type BH = BuildHasherDefault<FnvHasher>;
static TI_MAP: Lazy<CHashMap<u32, Arc<TaskInfo>, BH>> = Lazy::new(|| CHashMap::new(BH::default()));
static TID_ALLOC: Lazy<spin::Mutex<IdAllocator>> =
    Lazy::new(|| spin::Mutex::new(IdAllocator::new(0..=(NR_TASKS as u64 - 1))));

#[derive(Debug, Clone)]
pub struct Tid(u32, Arc<TaskInfo>);

impl Tid {
    #[inline]
    pub fn raw(&self) -> u32 {
        self.0
    }

    pub fn child(&self, hdl: Handle) -> Option<Child> {
        let _pree = super::PREEMPT.lock();
        self.handles().read().get::<Child>(hdl).cloned()
    }

    pub fn drop_child(&self, hdl: Handle) -> bool {
        let _pree = super::PREEMPT.lock();
        self.handles().write().drop_shared::<Child>(hdl)
    }
}

impl Deref for Tid {
    type Target = TaskInfo;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.1
    }
}

impl PartialEq for Tid {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && Arc::ptr_eq(&self.1, &other.1)
    }
}

fn next() -> Option<u32> {
    let mut alloc = TID_ALLOC.lock();
    alloc.allocate().map(|id| u32::try_from(id).unwrap())
}

pub fn allocate(ti: TaskInfo) -> Result<Tid, TaskInfo> {
    allocate_or(ti, |ti| ti)
}

pub fn allocate_or<F, R>(ti: TaskInfo, or_else: F) -> Result<Tid, R>
where
    F: FnOnce(TaskInfo) -> R,
{
    let _flags = PREEMPT.lock();
    match next() {
        Some(raw) => {
            let ti = Arc::new(ti);
            let old = TI_MAP.insert(raw, ti.clone());
            debug_assert!(old.is_none());
            Ok(Tid(raw, ti))
        }
        None => Err(or_else(ti)),
    }
}

pub fn deallocate(tid: &Tid) -> bool {
    let _flags = PREEMPT.lock();
    TI_MAP.remove(&tid.0).map_or(false, |_| {
        TID_ALLOC.lock().deallocate(u64::from(tid.0));
        true
    })
}

pub fn has_ti(tid: &Tid) -> bool {
    let _flags = PREEMPT.lock();
    TI_MAP.contains_key(&tid.0)
}
