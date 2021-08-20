pub mod ctx;
pub mod prio;
pub mod tid;

use crate::cpu::time::Instant;
use crate::cpu::CpuMask;
use crate::mem::space::Space;
use paging::LAddr;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::ptr::null_mut;
use core::time::Duration;
use spin::Lazy;

#[cfg(target_arch = "x86_64")]
pub use ctx::arch::DEFAULT_STACK_SIZE;
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

#[thread_local]
static IDLE: Lazy<Tid> = Lazy::new(|| {
      let ti = TaskInfo::new(
            *ROOT,
            format!("IDLE{}", unsafe { crate::cpu::id() }),
            Type::Kernel,
            crate::cpu::current_mask(),
            prio::IDLE,
      );

      let init = Init::new(ti, DEFAULT_STACK_SIZE, [0; 2]).expect("Failed to initialize IDLE");
      let tid = init.tid;

      let mut sched = super::SCHED.lock();
      unsafe { sched.push(init) };

      tid
});

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
pub enum InitError {
      Stack(&'static str),
      Tid,
}

#[derive(Debug)]
pub struct Init {
      tid: Tid,
      space: Space,
      kstack: Box<ctx::Kstack>,
}

impl Init {
      pub fn new(ti: TaskInfo, stack_size: usize, args: [u64; 2]) -> Result<Self, InitError> {
            let space = Space::new(ti.ty);

            let entry = ctx::Entry {
                  entry: LAddr::new(null_mut()), // TODO: set up entry point correctly.
                  stack: space.init_stack(stack_size).map_err(InitError::Stack)?,
                  args,
            };

            let kstack = ctx::Kstack::new(entry, ti.ty);

            let mut ti_map = tid::TI_MAP.lock();
            let tid = tid::next(&ti_map).map_or_else(
                  || {
                        let _ = space.clear_stack();
                        Err(InitError::Tid)
                  },
                  Ok,
            )?;
            ti_map.insert(tid, ti);

            Ok(Init { tid, space, kstack })
      }

      pub fn tid(&self) -> Tid {
            self.tid
      }
}

#[derive(Debug)]
pub enum RunningState {
      NotRunning,
      NeedResched,
      Running(Instant),
}

#[derive(Debug)]
pub struct Ready {
      tid: Tid,
      time_slice: Duration,

      space: Space,
      kstack: Box<ctx::Kstack>,
      ext_frame: Option<Box<ctx::ExtendedFrame>>,

      pub(super) cpu: usize,
      pub(super) running_state: RunningState,
}

impl Ready {
      pub fn from_init(init: Init, cpu: usize, time_slice: Duration) -> Self {
            let Init { tid, space, kstack } = init;
            Ready {
                  tid,
                  time_slice,
                  cpu,
                  space,
                  kstack,
                  ext_frame: None,
                  running_state: RunningState::NotRunning,
            }
      }

      pub fn time_slice(&self) -> Duration {
            self.time_slice
      }

      /// Save the context frame of the current task.
      ///
      /// # Safety
      ///
      /// The caller must ensure that `frame` points to a valid frame.
      pub unsafe fn save_arch(&mut self, frame: *const ctx::arch::Frame) {
            frame.copy_to(self.kstack.as_frame_mut(), 1);
      }

      /// Get the arch-specific context of the task.
      ///
      /// # Safety
      ///
      /// The caller must ensure that the pointer is used only in context switching.
      pub unsafe fn get_arch_context(&self) -> *const ctx::arch::Frame {
            self.kstack.as_frame()
      }

      pub fn space(&self) -> &Space {
            &self.space
      }
}

#[derive(Debug)]
pub struct Blocked {
      tid: Tid,
}

#[derive(Debug)]
pub struct Killed {
      tid: Tid,
}

#[derive(Debug)]
pub struct Dead {
      tid: Tid,
}
