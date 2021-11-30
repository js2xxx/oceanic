pub mod child;
pub mod ctx;
pub mod elf;
pub mod hdl;
pub mod idle;
pub mod prio;
pub mod sig;
pub mod syscall;
pub mod tid;

use alloc::{boxed::Box, format, string::String, sync::Arc};
use core::time::Duration;

use paging::LAddr;
use spin::Lazy;

#[cfg(target_arch = "x86_64")]
pub use self::ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
use self::{child::Child, sig::Signal};
pub use self::{
    elf::from_elf,
    hdl::{UserHandle, UserHandles},
    prio::Priority,
    tid::Tid,
};
use super::PREEMPT;
use crate::{
    cpu::{self, arch::KernelGs, time::Instant, CpuMask},
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
        signal: None,
    };

    tid::allocate(ti).expect("Failed to acquire a valid TID")
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
    from: Option<(Tid, Option<Arc<Child>>)>,
    name: String,
    ty: Type,
    affinity: CpuMask,
    prio: Priority,
    pub(in crate::sched) user_handles: UserHandles,
    signal: Option<Signal>,
}

impl TaskInfo {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> Type {
        self.ty
    }

    pub fn affinity(&self) -> crate::cpu::CpuMask {
        self.affinity.clone()
    }

    pub fn prio(&self) -> Priority {
        self.prio
    }

    pub fn take_signal(&mut self) -> Option<Signal> {
        self.signal.take()
    }

    pub fn replace_signal(&mut self, signal: Option<Signal>) -> Option<Signal> {
        match (signal, &mut self.signal) {
            (None, s) => {
                *s = None;
                None
            }
            (Some(signal), s) if s.is_none() => {
                *s = Some(signal);
                None
            }
            (Some(signal), s) => {
                (s.as_ref().unwrap() >= &signal).then(|| s.replace(signal).unwrap())
            }
        }
    }
}

#[derive(Debug)]
pub struct Init {
    tid: Tid,
    space: Arc<Space>,
    kstack: ctx::Kstack,
}

impl Init {
    fn new(
        tid: Tid,
        space: Arc<Space>,
        entry: LAddr,
        stack_size: usize,
        tls: Option<LAddr>,
        args: [u64; 2],
    ) -> Result<Self> {
        let entry = ctx::Entry {
            entry,
            stack: space
                .init_stack(stack_size)
                .map_err(TaskError::StackError)?,
            tls,
            args,
        };

        let kstack = ctx::Kstack::new(entry, tid.info().read().ty);

        Ok(Init { tid, space, kstack })
    }

    pub fn tid(&self) -> &Tid {
        &self.tid
    }
}

#[derive(Debug, Clone)]
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
    kstack: ctx::Kstack,
    ext_frame: Box<ctx::ExtendedFrame>,

    pub(super) cpu: usize,
    pub(super) running_state: RunningState,
    pub(super) runtime: Duration,
}

impl Ready {
    pub(in crate::sched) fn from_init(init: Init, cpu: usize, time_slice: Duration) -> Self {
        let Init { tid, space, kstack } = init;
        Ready {
            tid,
            time_slice,
            space,
            kstack,
            ext_frame: ctx::ExtendedFrame::zeroed(),
            cpu,
            running_state: RunningState::NotRunning,
            runtime: Duration::new(0, 0),
        }
    }

    pub(in crate::sched) fn unblock(blocked: Blocked, time_slice: Duration) -> Self {
        let Blocked {
            tid,
            space,
            kstack,
            ext_frame,
            cpu,
            runtime,
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
            runtime,
        }
    }

    pub(in crate::sched) fn block(this: Self, block_desc: &'static str) -> Blocked {
        let Ready {
            tid,
            space,
            kstack,
            ext_frame,
            cpu,
            runtime,
            ..
        } = this;
        Blocked {
            tid,
            space,
            kstack,
            ext_frame,
            cpu,
            block_desc,
            runtime,
        }
    }

    pub(in crate::sched) fn exit(this: Self, retval: usize) {
        let Ready { tid, kstack, .. } = this;
        let dead = Dead { tid, retval };
        destroy(dead);
        idle::CTX_DROPPER.push(kstack.into_ptr());
    }

    pub fn tid(&self) -> &Tid {
        &self.tid
    }

    pub fn time_slice(&self) -> Duration {
        self.time_slice
    }

    /// Save the context frame of the current task.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `frame` points to a valid frame.
    pub unsafe fn save_regs(&mut self) {
        debug_assert!(!matches!(self.running_state, RunningState::NotRunning));

        self.ext_frame.save();
    }

    pub unsafe fn load_intr(&self) {
        debug_assert!(!matches!(self.running_state, RunningState::NotRunning));

        let tss_rsp0 = self.kstack.top().val() as u64;
        KernelGs::update_tss_rsp0(tss_rsp0);
        crate::mem::space::set_current(self.space.clone());
        self.ext_frame.load();
        if !cpu::arch::in_intr() && self.tid.info().read().ty == Type::Kernel {
            KernelGs::reload();
        }
    }

    pub fn save_syscall_retval(&mut self, retval: usize) {
        debug_assert!(matches!(self.running_state, RunningState::Running(..)));

        self.kstack.task_frame_mut().set_syscall_retval(retval);
    }

    pub fn kframe(&self) -> *mut u8 {
        self.kstack.kframe_ptr()
    }

    pub fn kframe_mut(&mut self) -> *mut *mut u8 {
        self.kstack.kframe_ptr_mut()
    }

    pub fn space(&self) -> &Arc<Space> {
        &self.space
    }
}

#[derive(Debug)]
pub struct Blocked {
    tid: Tid,

    space: Arc<Space>,
    kstack: ctx::Kstack,
    ext_frame: Box<ctx::ExtendedFrame>,

    cpu: usize,
    block_desc: &'static str,
    runtime: Duration,
}

impl Blocked {
    pub fn tid(&self) -> &Tid {
        &self.tid
    }
}

#[derive(Debug)]
pub struct Dead {
    tid: Tid,
    retval: usize,
}

impl Dead {
    pub fn tid(&self) -> &Tid {
        &self.tid
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
    args: [u64; 2],
) -> Result<(Init, UserHandle)>
where
    F: FnOnce(&Space) -> Result<(LAddr, Option<LAddr>, usize)>,
{
    let (cur_tid, space) = super::SCHED
        .with_current(|cur| {
            (
                cur.tid.clone(),
                if dup_cur_space {
                    Space::clone(&cur.space, ty)
                } else {
                    Space::new(ty)
                },
            )
        })
        .ok_or(TaskError::NoCurrentTask)?;

    let (entry, tls, stack_size) = unsafe { with(&space, with_space) }?;

    let (tid, ret_wo) = {
        let pree = PREEMPT.lock();
        let cur_ti = cur_tid.info().upgradeable_read();

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

        let new_ti = TaskInfo {
            from: None,
            name,
            ty,
            affinity,
            prio,
            user_handles: UserHandles::new(),
            signal: None,
        };
        let tid = tid::allocate(new_ti).map_err(|_| TaskError::TidExhausted)?;

        let (ret_wo, child) = {
            let mut cur_ti = cur_ti.upgrade();
            let child = Arc::new(Child::new(tid.clone()));
            (cur_ti.user_handles.insert(child.clone()).unwrap(), child)
        };
        drop(pree);

        tid.info().write().from = Some((cur_tid, Some(child)));
        (tid, ret_wo)
    };

    Init::new(tid, space, entry, stack_size, tls, args).map(|task| (task, ret_wo))
}

pub fn create_fn(
    name: Option<String>,
    stack_size: usize,
    func: LAddr,
    arg: *mut u8,
) -> Result<(Init, UserHandle)> {
    let (name, ty, affinity, prio) = super::SCHED
        .with_current(|cur| {
            let ti = cur.tid.info().read();
            (
                name.unwrap_or(format!("{}.func{:?}", ti.name, *func)),
                ti.ty,
                ti.affinity.clone(),
                ti.prio,
            )
        })
        .ok_or(TaskError::NoCurrentTask)?;

    create_with_space(
        name,
        ty,
        affinity,
        prio,
        true,
        |_| Ok((func, None, stack_size)),
        [arg as u64, 0],
    )
}

pub(super) fn destroy(task: Dead) {
    if let Some((_, Some(child))) = {
        tid::deallocate(&task.tid);
        let ti = task.tid.info().read();
        ti.from.clone()
    } {
        let _ = child.cell().replace(task.retval);
    }
}
