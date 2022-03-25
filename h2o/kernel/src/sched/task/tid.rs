use core::{hash::BuildHasherDefault, num::NonZeroU32, ops::Deref};

use archop::Azy;
use collection_ex::{CHashMap, FnvHasher, IdAllocator};
use sv_call::Feature;

use super::{hdl::DefaultFeature, TaskInfo};
use crate::sched::{Arsc, PREEMPT};

pub const NR_TASKS: usize = 65536;

type BH = BuildHasherDefault<FnvHasher>;
static TI_MAP: Azy<CHashMap<u32, Arsc<TaskInfo>, BH>> = Azy::new(|| CHashMap::new(BH::default()));
static TID_ALLOC: Azy<spin::Mutex<IdAllocator>> =
    Azy::new(|| spin::Mutex::new(IdAllocator::new(0..=(NR_TASKS as u64 - 1))));

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Tid {
    raw: NonZeroU32,
    ti: Arsc<TaskInfo>,
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
}

impl PartialEq for Tid {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw && Arsc::ptr_eq(&self.ti, &other.ti)
    }
}

unsafe impl DefaultFeature for Tid {
    fn default_features() -> Feature {
        Feature::SEND
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
            let ti = Arsc::try_new(ti)?;
            let old = TI_MAP.insert(raw.get(), ti.clone());
            debug_assert!(old.is_none());
            Ok(Tid { raw, ti })
        }
        None => Err(sv_call::Error::ENOSPC),
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
