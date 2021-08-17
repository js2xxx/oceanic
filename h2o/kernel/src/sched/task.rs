pub mod prio;

use alloc::string::String;

use crate::cpu::CpuMask;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tid(u32);

#[derive(Debug)]
pub struct TaskInfo {
      name: String,
      affinity: CpuMask,
      prio: prio::Priority,
}

#[derive(Debug)]
pub struct Init {
      tid: Tid,
      receiver: Option<Tid>,
}

#[derive(Debug)]
pub struct Ready {
      tid: Tid,
      receiver: Option<Tid>,
}

#[derive(Debug)]
pub struct Blocked {
      tid: Tid,
      receiver: Option<Tid>,
}

#[derive(Debug)]
pub struct Dead {
      tid: Tid,
      receiver: Tid,
      retval: u64,
}
