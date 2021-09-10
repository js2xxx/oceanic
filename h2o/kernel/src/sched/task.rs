pub mod ctx;
pub mod elf;
pub mod hdl;
pub mod idle;
pub mod prio;
pub mod tid;

pub use elf::from_elf;
pub use hdl::{UserHandle, UserHandles};

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

use super::wait::{WaitCell, WaitObject};

static ROOT: Lazy<Tid> = Lazy::new(|| {
      let ti = TaskInfo {
            from: None,
            name: String::from("ROOT"),
            ty: Type::Kernel,
            affinity: crate::cpu::all_mask(),
            prio: prio::DEFAULT,
            user_handles: UserHandles::new(),
      };

      let mut ti_map = tid::TI_MAP.lock();
      let tid = tid::next(&ti_map).expect("Failed to acquire a valid TID");
      ti_map.insert(tid, ti);

      tid
});

#[derive(Debug)]
pub enum TaskError {
      Permission,
      NotSupported(u32),
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
      from: Option<(Tid, UserHandle)>,
      name: String,
      ty: Type,
      affinity: CpuMask,
      prio: Priority,
      user_handles: UserHandles,
}

impl TaskInfo {
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
      intr_stack: Box<ctx::Kstack>,
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

            let (intr_stack, rem) = ctx::Kstack::new(entry, ti.ty);

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

            Ok((
                  Init {
                        tid,
                        space,
                        intr_stack,
                  },
                  rem,
            ))
      }

      pub fn tid(&self) -> Tid {
            self.tid
      }
}

#[derive(Debug, Clone)]
pub enum RunningState {
      NotRunning,
      NeedResched,
      Running(Instant),
      Drowsy(Arc<WaitObject>, &'static str),
      Dying(usize),
}

#[derive(Debug)]
pub struct Ready {
      tid: Tid,
      time_slice: Duration,

      space: Arc<Space>,
      intr_stack: Box<ctx::Kstack>,
      syscall_stack: Box<ctx::Kstack>,
      ext_frame: Box<ctx::ExtendedFrame>,

      pub(super) cpu: usize,
      pub(super) running_state: RunningState,
}

impl Ready {
      pub(in crate::sched) fn from_init(init: Init, cpu: usize, time_slice: Duration) -> Self {
            let Init {
                  tid,
                  space,
                  intr_stack,
            } = init;
            Ready {
                  tid,
                  time_slice,
                  space,
                  intr_stack,
                  syscall_stack: ctx::Kstack::new_syscall(),
                  ext_frame: box unsafe { core::mem::zeroed() },
                  cpu,
                  running_state: RunningState::NotRunning,
            }
      }

      pub(in crate::sched) fn from_blocked(blocked: Blocked, time_slice: Duration) -> Self {
            let Blocked {
                  tid,
                  space,
                  intr_stack,
                  syscall_stack,
                  ext_frame,
                  cpu,
                  ..
            } = blocked;
            Ready {
                  tid,
                  time_slice,
                  space,
                  intr_stack,
                  syscall_stack,
                  ext_frame,
                  cpu,
                  running_state: RunningState::NotRunning,
            }
      }

      pub(in crate::sched) fn into_blocked(this: Self) {
            let Ready {
                  tid,
                  space,
                  intr_stack,
                  syscall_stack,
                  ext_frame,
                  cpu,
                  running_state,
                  ..
            } = this;
            let (wo, block_desc) = match running_state {
                  RunningState::Drowsy(wo, block_desc) => (wo, block_desc),
                  _ => unreachable!("Blocked task unblockable"),
            };
            let blocked = Blocked {
                  tid,
                  space,
                  intr_stack,
                  syscall_stack,
                  ext_frame,
                  cpu,
                  block_desc,
            };
            wo.wait_queue.lock().push_back(blocked);
      }

      pub(in crate::sched) fn into_dead(this: Self) -> Dead {
            let Ready {
                  tid, running_state, ..
            } = this;
            let retval = match running_state {
                  RunningState::Dying(retval) => retval,
                  _ => unreachable!("Dead task not dying"),
            };
            Dead { tid, retval }
      }

      pub fn tid(&self) -> Tid {
            self.tid
      }

      pub fn time_slice(&self) -> Duration {
            self.time_slice
      }

      /// Save the context frame of the current task.
      ///
      /// # Safety
      ///
      /// The caller must ensure that `frame` points to a valid frame.
      pub unsafe fn save_intr(
            &mut self,
            frame: *const ctx::arch::Frame,
      ) -> *const ctx::arch::Frame {
            frame.copy_to(self.intr_stack.task_frame_mut(), 1);
            self.ext_frame.save();
            self.intr_stack.task_frame()
      }

      pub unsafe fn sync_syscall(
            &mut self,
            frame: *const ctx::arch::Frame,
      ) -> *const ctx::arch::Frame {
            frame.copy_to(self.syscall_stack.task_frame_mut(), 1);
            self.syscall_stack.task_frame()
      }

      pub fn save_syscall_retval(&mut self, retval: usize) {
            self.intr_stack.task_frame_mut().set_syscall_retval(retval);
      }

      pub unsafe fn load_intr(&self, reload_all: bool) -> *const ctx::arch::Frame {
            if reload_all {
                  crate::mem::space::set_current(self.space.clone());
                  self.ext_frame.load();
            }
            self.intr_stack.task_frame()
      }

      pub fn space(&self) -> &Arc<Space> {
            &self.space
      }
}

#[derive(Debug)]
pub struct Blocked {
      tid: Tid,

      space: Arc<Space>,
      intr_stack: Box<ctx::Kstack>,
      syscall_stack: Box<ctx::Kstack>,
      ext_frame: Box<ctx::ExtendedFrame>,

      cpu: usize,
      block_desc: &'static str,
}

#[derive(Debug)]
pub struct Killed {
      tid: Tid,
}

#[derive(Debug)]
pub struct Dead {
      tid: Tid,
      retval: usize,
}

impl Dead {
      pub fn tid(&self) -> Tid {
            self.tid
      }

      pub fn retval(&self) -> usize {
            self.retval
      }
}

pub(super) fn init() {
      Lazy::force(&idle::IDLE);
}

fn create_with_space<F>(
      name: String,
      ty: Type,
      affinity: CpuMask,
      prio: Priority,
      with_space: F,
      args: &[u64],
) -> Result<(Init, UserHandle, Option<&[u64]>)>
where
      F: FnOnce(&Space) -> Result<(LAddr, Option<LAddr>, usize)>,
{
      let (cur_tid, space) = {
            let sched = super::SCHED.lock();
            let cur = sched.current().ok_or(TaskError::NoCurrentTask)?;
            (cur.tid, cur.space.duplicate(ty))
      };

      let (entry, tls, stack_size) = unsafe { with(&space, with_space) }?;

      let (ti, ret_wo) = {
            let mut ti_map = tid::TI_MAP.lock();
            let cur_ti = ti_map.get_mut(&cur_tid).unwrap();

            let ret_wo = cur_ti.user_handles.insert(WaitCell::<usize>::new());

            let ty = match ty {
                  Type::Kernel => cur_ti.ty,
                  Type::User => {
                        if ty == Type::Kernel {
                              return Err(TaskError::Permission);
                        } else {
                              Type::User
                        }
                  }
            };
            let prio = prio.min(cur_ti.prio);

            (
                  TaskInfo {
                        from: Some((cur_tid, ret_wo)),
                        name,
                        ty,
                        affinity,
                        prio,
                        user_handles: UserHandles::new(),
                  },
                  ret_wo,
            )
      };

      Init::new(ti, space, entry, stack_size, tls, args).map(|(task, rem)| (task, ret_wo, rem))
}

pub fn create_fn(
      name: Option<String>,
      stack_size: usize,
      func: LAddr,
      arg: *mut u8,
) -> Result<(Init, UserHandle)> {
      let (name, ty, affinity, prio) = {
            let cur_tid = super::SCHED
                  .lock()
                  .current()
                  .ok_or(TaskError::NoCurrentTask)?
                  .tid;
            let ti_map = tid::TI_MAP.lock();
            let ti = ti_map.get(&cur_tid).unwrap();
            (
                  name.unwrap_or(format!("{}.func{:?}", ti.name, *func)),
                  ti.ty,
                  ti.affinity.clone(),
                  ti.prio,
            )
      };
      create_with_space(
            name,
            ty,
            affinity,
            prio,
            |_| Ok((func, None, stack_size)),
            &[arg as u64],
      )
      .map(|(task, ret_wo, _)| (task, ret_wo))
}

pub(super) fn destroy(task: Dead) {
      let mut ti_map = tid::TI_MAP.lock();
      let TaskInfo { from, .. } = ti_map.remove(&task.tid).unwrap();

      if let Some(cell) = from.and_then(|(from_tid, ret_wo_hdl)| {
            ti_map.get(&from_tid)
                  .and_then(|parent| parent.user_handles.get::<WaitCell<usize>>(ret_wo_hdl))
      }) {
            let _ = cell.replace(task.retval);
      }
}

pub mod syscall {
      use solvent::*;
      #[syscall]
      pub fn exit(retval: usize) {
            {
                  let mut sched = crate::sched::SCHED.lock();
                  if let Some(cur) = sched.current_mut() {
                        cur.running_state = super::RunningState::Dying(retval);
                  }
            }
            loop {
                  unsafe { archop::pause() };
            }
      }

      #[syscall]
      pub fn task_fn(name: *mut u8, stack_size: usize, func: *mut u8, arg: *mut u8) -> usize {
            extern "C" {
                  fn strlen(s: *const u8) -> usize;
            }
            use crate::alloc::string::ToString;

            let name = if !name.is_null() {
                  unsafe {
                        let slice = core::slice::from_raw_parts(name, strlen(name));
                        Some(core::str::from_utf8(slice)
                              .map_err(|_| Error(EINVAL))?
                              .to_string())
                  }
            } else {
                  None
            };

            let (task, ret_wo) = super::create_fn(name, stack_size, paging::LAddr::new(func), arg)
                  .map_err(|te| {
                        Error(match te {
                              super::TaskError::Permission => EPERM,
                              super::TaskError::NotSupported(_) => ENOSPC,
                              super::TaskError::InvalidFormat => EINVAL,
                              super::TaskError::Memory(_) => ENOMEM,
                              super::TaskError::NoCurrentTask => EFAULT,
                              super::TaskError::TidExhausted => EFAULT,
                              super::TaskError::StackError(_) => ENOMEM,
                              super::TaskError::Other(_) => EFAULT,
                        })
                  })?;
            crate::sched::SCHED.lock().push(task);
            Ok(ret_wo.raw())
      }
}
