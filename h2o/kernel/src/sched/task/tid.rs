use super::TaskInfo;

use alloc::collections::BTreeMap;
use spin::{Mutex, MutexGuard};

pub const NR_TASKS: usize = 65536;

pub(super) static TI_MAP: Mutex<BTreeMap<Tid, TaskInfo>> = Mutex::new(BTreeMap::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tid(u32);

pub(super) fn next(ti_map: &MutexGuard<BTreeMap<Tid, TaskInfo>>) -> Option<Tid> {
      (1..=NR_TASKS as u32).find_map(|idx| {
            if ti_map.contains_key(&Tid(idx)) {
                  Some(Tid(idx))
            } else {
                  None
            }
      })
}
