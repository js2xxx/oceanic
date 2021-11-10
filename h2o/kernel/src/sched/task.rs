pub mod ctx;
pub mod elf;
pub mod hdl;
pub mod idle;
pub mod prio;
pub mod tid;

use alloc::{boxed::Box, format, string::String, sync::Arc};
use core::time::Duration;

use paging::LAddr;
use spin::Lazy;

#[cfg(target_arch = "x86_64")]
pub use self::ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
pub use self::{
    elf::from_elf,
    hdl::{UserHandle, UserHandles},
    prio::Priority,
    tid::Tid,
};
use super::wait::{WaitCell, WaitObject};
use crate::{
    cpu::{time::Instant, CpuMask},
    mem::space::{with, Space, SpaceError},
};

static ROOT: Lazy<Tid> = Lazy::new(|| {
    let ti = TaskInfo {
        from: None,
        name: String::from("ROOT"),
        ty: Type::Kernel,
        affinity: crate::cpu::all_mask(),
        prio: prio::DEFAULT,
        user_handles: UserHandles::new(),
    };

    tid::alloc_insert(ti).expect("Failed to acquire a valid TID")
});

#[derive(Debug)]
pub enum TaskError {
    Permission,
    NotSupported(u32),
    InvalidFormat,
    Memory(SpaceError),
    NoCurrentTask,
    TidExhausted,
    StackError(SpaceError),
    Other(&'static str),
}

impl Into<solvent::Error> for TaskError {
    fn into(self) -> solvent::Error {
        use solvent::*;
        Error(match self {
            TaskError::Permission => EPERM,
            TaskError::NotSupported(_) => EPERM,
            TaskError::InvalidFormat => EINVAL,
            TaskError::Memory(_) => ENOMEM,
            TaskError::NoCurrentTask => ESRCH,
            TaskError::TidExhausted => EFAULT,
            TaskError::StackError(_) => ENOMEM,
            TaskError::Other(_) => EFAULT,
        })
    }
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

        let tid = tid::alloc_insert_or(ti, |_ti| {
            let _ = space.clear_stack();
            TaskError::TidExhausted
        })?;

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
        wo.wait_queue.push(blocked);
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
    pub unsafe fn save_intr(&mut self, frame: *const ctx::arch::Frame) -> *const ctx::arch::Frame {
        assert!(
            !matches!(self.running_state, RunningState::NotRunning),
            "{:?}",
            self.running_state
        );

        frame.copy_to(self.intr_stack.task_frame_mut(), 1);
        self.ext_frame.save();
        self.intr_stack.task_frame()
    }

    pub unsafe fn load_intr(&self, reload_all: bool) -> *const ctx::arch::Frame {
        assert!(
            !matches!(self.running_state, RunningState::NotRunning),
            "{:?}",
            self.running_state
        );

        if reload_all {
            crate::mem::space::set_current(self.space.clone());
            self.ext_frame.load();
        }
        self.intr_stack.task_frame()
    }

    pub unsafe fn sync_syscall(
        &mut self,
        frame: *const ctx::arch::Frame,
    ) -> *const ctx::arch::Frame {
        assert!(
            !matches!(self.running_state, RunningState::NotRunning),
            "{:?}",
            self.running_state
        );

        frame.copy_to(self.syscall_stack.task_frame_mut(), 1);
        self.syscall_stack.task_frame()
    }

    pub fn save_syscall_retval(&mut self, retval: usize) {
        assert!(matches!(self.running_state, RunningState::Running(..)));

        self.syscall_stack
            .task_frame_mut()
            .set_syscall_retval(retval);
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
    dup_cur_space: bool,
    with_space: F,
    args: &[u64],
) -> Result<(Init, UserHandle, Option<&[u64]>)>
where
    F: FnOnce(&Space) -> Result<(LAddr, Option<LAddr>, usize)>,
{
    let (cur_tid, space) = super::SCHED
        .with_current(|cur| {
            (
                cur.tid,
                if dup_cur_space {
                    Space::clone(&cur.space, ty)
                } else {
                    Space::new(ty)
                },
            )
        })
        .ok_or(TaskError::NoCurrentTask)?;

    let (entry, tls, stack_size) = unsafe { with(&space, with_space) }?;

    let (ti, ret_wo) = {
        let mut cur_ti = tid::get_mut(&cur_tid).unwrap();

        let ret_wo = cur_ti.user_handles.insert(WaitCell::<usize>::new()).unwrap();

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
            .with_current(|cur| cur.tid)
            .ok_or(TaskError::NoCurrentTask)?;
        let ti = tid::get(&cur_tid).unwrap();
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
        true,
        |_| Ok((func, None, stack_size)),
        &[arg as u64],
    )
    .map(|(task, ret_wo, _)| (task, ret_wo))
}

pub(super) fn destroy(task: Dead) {
    if let Some(cell) = {
        let TaskInfo { from, .. } = tid::remove(&task.tid).unwrap();
        from.and_then(|(from_tid, ret_wo_hdl)| {
            tid::get(&from_tid).and_then(|parent| {
                parent
                    .user_handles
                    .get::<Arc<WaitCell<usize>>>(ret_wo_hdl)
                    .cloned()
            })
        })
    } {
        let _ = cell.replace(task.retval);
    }
}

pub mod syscall {
    use solvent::*;

    #[syscall]
    pub fn task_exit(retval: usize) {
        crate::sched::SCHED.with_current(|cur| {
            cur.running_state = super::RunningState::Dying(retval);
        });

        loop {
            core::hint::spin_loop();
        }
    }

    #[syscall]
    pub fn task_fn(name: *mut u8, stack_size: usize, func: *mut u8, arg: *mut u8) -> u32 {
        extern "C" {
            fn strlen(s: *const u8) -> usize;
        }
        use crate::alloc::string::ToString;

        let name = if !name.is_null() {
            unsafe {
                let slice = core::slice::from_raw_parts(name, strlen(name));
                Some(
                    core::str::from_utf8(slice)
                        .map_err(|_| Error(EINVAL))?
                        .to_string(),
                )
            }
        } else {
            None
        };

        let (task, ret_wo) = super::create_fn(name, stack_size, paging::LAddr::new(func), arg)
            .map_err(Into::into)?;
        crate::sched::SCHED.push(task);
        Ok(ret_wo.raw())
    }

    #[syscall]
    pub fn task_join(wc_raw: u32) -> usize {
        use core::num::NonZeroU32;

        use crate::sched::wait::WaitCell;
        let wc_hdl = super::UserHandle::new(NonZeroU32::new(wc_raw).ok_or(Error(EINVAL))?);

        let cur_tid = crate::sched::SCHED
            .with_current(|cur| cur.tid)
            .ok_or(Error(ESRCH))?;

        let wc = {
            let ti = super::tid::get(&cur_tid).ok_or(Error(ESRCH))?;
            ti.user_handles
                .get::<alloc::sync::Arc<WaitCell<usize>>>(wc_hdl)
                .ok_or(Error(ECHILD))?
                .clone()
        };

        Ok(wc.take("task_join"))
    }
}
