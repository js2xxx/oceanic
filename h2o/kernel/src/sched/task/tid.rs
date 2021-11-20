use alloc::sync::Arc;
use core::{hash::BuildHasherDefault, ptr};

use archop::{IntrRwLock, IntrState};
use collection_ex::{CHashMap, FnvHasher, IdAllocator};
use spin::Lazy;

use super::TaskInfo;

pub const NR_TASKS: usize = 65536;

type BH = BuildHasherDefault<FnvHasher>;
static TI_MAP: Lazy<CHashMap<u32, Arc<IntrRwLock<TaskInfo>>, BH>> =
    Lazy::new(|| CHashMap::new(BH::default()));
static TID_ALLOC: Lazy<spin::Mutex<IdAllocator>> =
    Lazy::new(|| spin::Mutex::new(IdAllocator::new(0..=(NR_TASKS as u64 - 1))));

#[derive(Debug, Clone)]
pub struct Tid(u32, Arc<IntrRwLock<TaskInfo>>);

impl Tid {
    pub fn raw(&self) -> u32 {
        self.0
    }

    pub fn info(&self) -> &IntrRwLock<TaskInfo> {
        &*self.1
    }
}

impl PartialEq for Tid {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && ptr::eq(self.1.as_mut_ptr(), other.1.as_mut_ptr())
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
    let _flags = IntrState::lock();
    match next() {
        Some(raw) => {
            let ti = Arc::new(IntrRwLock::new(ti));
            let old = TI_MAP.insert(raw, ti.clone());
            debug_assert!(old.is_none());
            Ok(Tid(raw, ti))
        }
        None => Err(or_else(ti)),
    }
}

pub fn deallocate(tid: &Tid) -> bool {
    let _flags = IntrState::lock();
    TI_MAP.remove(&tid.0).map_or(false, |_| {
        TID_ALLOC.lock().deallocate(u64::from(tid.0));
        true
    })
}

pub fn has_ti(tid: &Tid) -> bool {
    let _flags = IntrState::lock();
    TI_MAP.contains_key(&tid.0)
}

pub fn init() {
    Lazy::force(&TI_MAP);
}
