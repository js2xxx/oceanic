use core::{
    hash::BuildHasherDefault,
    ops::{Deref, DerefMut},
};

use archop::IntrState;
use collection_ex::{CHashMap, FnvHasher, IdAllocator};
use spin::Lazy;

use super::TaskInfo;

pub const NR_TASKS: usize = 65536;

type BH = BuildHasherDefault<FnvHasher>;
static TI_MAP: Lazy<CHashMap<Tid, TaskInfo, BH>> = Lazy::new(|| CHashMap::new(BH::default()));
static TID_ALLOC: Lazy<spin::Mutex<IdAllocator>> =
    Lazy::new(|| spin::Mutex::new(IdAllocator::new(0..=(NR_TASKS as u64 - 1))));

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tid(u32);

pub struct ReadGuard<'a> {
    _intr: IntrState,
    inner: collection_ex::chash_map::ReadGuard<'a, Tid, TaskInfo, BH>,
}

impl<'a> Deref for ReadGuard<'a> {
    type Target = TaskInfo;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub struct WriteGuard<'a> {
    _intr: IntrState,
    inner: collection_ex::chash_map::WriteGuard<'a, Tid, TaskInfo, BH>,
}

impl<'a> Deref for WriteGuard<'a> {
    type Target = TaskInfo;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for WriteGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

fn next() -> Option<Tid> {
    let mut alloc = TID_ALLOC.lock();
    alloc.alloc().map(|id| Tid(u32::try_from(id).unwrap()))
}

pub fn alloc_insert(ti: TaskInfo) -> Result<Tid, TaskInfo> {
    alloc_insert_or(ti, |ti| ti)
}

pub fn alloc_insert_or<F, R>(ti: TaskInfo, or_else: F) -> Result<Tid, R>
where
    F: FnOnce(TaskInfo) -> R,
{
    let _flags = IntrState::lock();
    match next() {
        Some(tid) => {
            let old = TI_MAP.insert(tid, ti);
            debug_assert!(old.is_none());
            Ok(tid)
        }
        None => Err(or_else(ti)),
    }
}

pub fn insert(tid: Tid, ti: TaskInfo) -> Option<TaskInfo> {
    let _flags = IntrState::lock();
    TI_MAP.insert(tid, ti).map(|r| r.1)
}

pub fn remove(tid: &Tid) -> Option<TaskInfo> {
    let _flags = IntrState::lock();
    TI_MAP.remove(tid).map(|ret| {
        TID_ALLOC.lock().dealloc(u64::from(tid.0));
        ret
    })
}

pub fn get<'a>(tid: &'a Tid) -> Option<ReadGuard<'a>> {
    let flags = IntrState::lock();
    let inner = TI_MAP.get(tid);
    inner.map(|inner| ReadGuard {
        _intr: flags,
        inner,
    })
}

pub fn get_mut<'a>(tid: &'a Tid) -> Option<WriteGuard<'a>> {
    let flags = IntrState::lock();
    let inner = TI_MAP.get_mut(tid);
    inner.map(|inner| WriteGuard {
        _intr: flags,
        inner,
    })
}

pub fn has_ti(tid: &Tid) -> bool {
    let _flags = IntrState::lock();
    TI_MAP.contains_key(tid)
}

pub fn init() {
    Lazy::force(&TI_MAP);
}
