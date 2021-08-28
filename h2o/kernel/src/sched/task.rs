pub mod ctx;
pub mod elf;
pub mod idle;
pub mod prio;
pub mod tid;

pub use elf::from_elf;

use crate::cpu::time::Instant;
use crate::cpu::CpuMask;
use crate::mem::space::{with, Space};
use paging::LAddr;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use core::time::Duration;
use spin::Lazy;

#[cfg(target_arch = "x86_64")]
pub use ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
pub use prio::Priority;
pub use tid::Tid;

static ROOT: Lazy<Tid> = Lazy::new(|| {
      let ti = TaskInfo {
            from: None,
            name: String::from("ROOT"),
            ty: Type::Kernel,
            affinity: crate::cpu::all_mask(),
            prio: prio::DEFAULT,
      };

      let mut ti_map = tid::TI_MAP.lock();
      let tid = tid::next(&ti_map).expect("Failed to acquire a valid TID");
      ti_map.insert(tid, ti);

      tid
});

#[derive(Debug)]
pub enum TaskError {
      NotSupported,
      InvalidFormat,
      Memory(&'static str),
      NoCurrentTask,
      TidExhausted,
      StackError(&'static str),
      Other(&'static str),
}

pub type Result<T> = core::result::Result<T, TaskError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
      Kernel,
      User,
}

#[derive(Debug)]
pub struct TaskInfo {
      from: Option<Tid>,
      name: String,
      ty: Type,
      affinity: CpuMask,
      prio: Priority,
}

impl TaskInfo {
      pub fn new(from: Tid, name: String, ty: Type, affinity: CpuMask, prio: Priority) -> Self {
            TaskInfo {
                  from: Some(from),
                  name,
                  ty,
                  affinity,
                  prio,
            }
      }

      pub fn name(&self) -> &str {
            &self.name
      }

      pub fn affinity(&self) -> crate::cpu::CpuMask {
            self.affinity.clone()
      }

      pub fn ty(&self) -> Type {
            self.ty
      }
}

#[derive(Debug)]
pub struct Init {
      tid: Tid,
      space: Arc<Space>,
      kstack: Box<ctx::Kstack>,
}

impl Init {
      fn new(
            ti: TaskInfo,
            space: Arc<Space>,
            entry: LAddr,
            stack_size: usize,
            tls: Option<LAddr>,
            args: &[u64],
      ) -> Result<(Self, Option<&[u64]>)> {
            let entry = ctx::Entry {
                  entry,
                  stack: space
                        .init_stack(stack_size)
                        .map_err(TaskError::StackError)?,
                  tls,
                  args,
            };

            let (kstack, rem) = ctx::Kstack::new(entry, ti.ty);

            let mut ti_map = tid::TI_MAP.lock();
            let tid = tid::next(&ti_map).map_or_else(
                  || {
                        let _ = space.clear_stack();
                        Err(TaskError::TidExhausted)
                  },
                  Ok,
            )?;
            ti_map.insert(tid, ti);
            drop(ti_map);

            Ok((Init { tid, space, kstack }, rem))
      }

      pub fn tid(&self) -> Tid {
            self.tid
      }
}

#[derive(Debug, Clone, Copy)]
pub enum RunningState {
      NotRunning,
      NeedResched,
      Running(Instant),
}

#[derive(Debug)]
pub struct Ready {
      tid: Tid,
      time_slice: Duration,

      space: Arc<Space>,
      kstack: Box<ctx::Kstack>,
      ext_frame: Box<ctx::ExtendedFrame>,

      pub(super) cpu: usize,
      pub(super) running_state: RunningState,
}

impl Ready {
      pub(in crate::sched) fn from_init(init: Init, cpu: usize, time_slice: Duration) -> Self {
            let Init { tid, space, kstack } = init;
            Ready {
                  tid,
                  time_slice,
                  space,
                  kstack,
                  ext_frame: box unsafe { core::mem::zeroed() },
                  cpu,
                  running_state: RunningState::NotRunning,
            }
      }

      pub(in crate::sched) fn from_blocked(blocked: Blocked, time_slice: Duration) -> Self {
            let Blocked {
                  tid,
                  space,
                  kstack,
                  ext_frame,
                  cpu,
                  ..
            } = blocked;
            Ready {
                  tid,
                  time_slice,
                  space,
                  kstack,
                  ext_frame,
                  cpu,
                  running_state: RunningState::NotRunning,
            }
      }

      pub(in crate::sched) fn into_blocked(this: Self, block_desc: String) -> Blocked {
            let Ready {
                  tid,
                  space,
                  kstack,
                  ext_frame,
                  cpu,
                  ..
            } = this;
            Blocked {
                  tid,
                  space,
                  kstack,
                  ext_frame,
                  cpu,
                  block_desc,
            }
      }

      pub(in crate::sched) fn into_dead(this: Self, retval: u64) -> Dead {
            let Ready { tid, .. } = this;
            Dead { tid, retval }
      }

      pub fn tid(&self) -> Tid {
            self.tid
      }

      pub fn time_slice(&self) -> Duration {
            self.time_slice
      }

      pub unsafe fn save_ext_frame(&mut self) {
            self.ext_frame.save()
      }

      pub unsafe fn load_ext_frame(&self) {
            self.ext_frame.load()
      }

      /// Save the context frame of the current task.
      ///
      /// # Safety
      ///
      /// The caller must ensure that `frame` points to a valid frame.
      pub unsafe fn save_arch(&mut self, frame: *const ctx::arch::Frame) {
            frame.copy_to(self.kstack.as_frame_mut(), 1);
      }

      pub fn save_syscall_retval(&mut self, retval: usize) {
            self.kstack.as_frame_mut().set_syscall_retval(retval);
      }

      /// Get the arch-specific context of the task.
      ///
      /// # Safety
      ///
      /// The caller must ensure that the pointer is used only in context switching.
      pub unsafe fn get_arch_context(&self) -> *const ctx::arch::Frame {
            self.kstack.as_frame()
      }

      pub fn space(&self) -> &Arc<Space> {
            &self.space
      }
}

#[derive(Debug)]
pub struct Blocked {
      tid: Tid,

      space: Arc<Space>,
      kstack: Box<ctx::Kstack>,
      ext_frame: Box<ctx::ExtendedFrame>,

      cpu: usize,
      block_desc: String,
}

#[derive(Debug)]
pub struct Killed {
      tid: Tid,
}

#[derive(Debug)]
pub struct Dead {
      tid: Tid,
      retval: u64,
}

impl Dead {
      pub fn tid(&self) -> Tid {
            self.tid
      }

      pub fn retval(&self) -> u64 {
            self.retval
      }
}

pub(super) fn init() {
      Lazy::force(&idle::IDLE);
}

pub fn create<F>(
      name: String,
      ty: Type,
      affinity: CpuMask,
      prio: Priority,
      with_space: F,
      args: &[u64],
) -> Result<(Init, Option<&[u64]>)>
where
      F: FnOnce(&Space) -> Result<(LAddr, Option<LAddr>, usize)>,
{
      let (cur_tid, space) = {
            let sched = super::SCHED.lock();
            let cur = sched.current().ok_or(TaskError::NoCurrentTask)?;
            (cur.tid, cur.space.duplicate(ty))
      };

      let (entry, tls, stack_size) = unsafe { with(&space, with_space) }?;

      let ti = {
            let ti_map = tid::TI_MAP.lock();
            let cur_ti = ti_map.get(&cur_tid).unwrap();

            let ty = match ty {
                  Type::Kernel => cur_ti.ty,
                  Type::User => Type::User,
            };
            let prio = prio.min(cur_ti.prio);

            TaskInfo {
                  from: Some(cur_tid),
                  name,
                  ty,
                  affinity,
                  prio,
            }
      };

      Init::new(ti, space, entry, stack_size, tls, args)
}
