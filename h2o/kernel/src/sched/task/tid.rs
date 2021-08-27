use super::TaskInfo;

use alloc::collections::BTreeMap;
use archop::{IntrMutex, IntrMutexGuard};

pub const NR_TASKS: usize = 65536;

pub(in crate::sched) static TI_MAP: IntrMutex<BTreeMap<Tid, TaskInfo>> =
      IntrMutex::new(BTreeMap::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tid(u32);

pub(super) fn next(ti_map: &IntrMutexGuard<BTreeMap<Tid, TaskInfo>>) -> Option<Tid> {
      (1..=NR_TASKS as u32).find_map(|idx| {
            if !ti_map.contains_key(&Tid(idx)) {
                  Some(Tid(idx))
            } else {
                  None
            }
      })
}
