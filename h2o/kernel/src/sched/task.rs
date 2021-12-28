pub mod child;
pub mod ctx;
mod elf;
mod hdl;
pub mod idle;
pub mod prio;
pub mod sig;
mod syscall;
pub mod tid;

use alloc::{boxed::Box, format, string::String, sync::Arc};
use core::{cell::UnsafeCell, time::Duration};

use paging::LAddr;
use solvent::Handle;
use spin::{Lazy, Mutex, RwLock};

#[cfg(target_arch = "x86_64")]
pub use self::ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
use self::{child::Child, sig::Signal};
pub use self::{elf::from_elf, hdl::HandleMap, prio::Priority, tid::Tid};
use super::{ipc::Channel, PREEMPT};
use crate::{
    cpu::{self, arch::KERNEL_GS, time::Instant, CpuLocalLazy, CpuMask},
    mem::space::{Space, SpaceError},
};

static ROOT: Lazy<Tid> = Lazy::new(|| {
    let ti = TaskInfo {
        from: UnsafeCell::new(None),
        name: String::from("ROOT"),
        ty: Type::Kernel,
        affinity: crate::cpu::all_mask(),
        prio: prio::DEFAULT,
        handles: RwLock::new(HandleMap::new()),
        signal: Mutex::new(None),
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
    from: UnsafeCell<Option<(Tid, Option<Child>)>>,
    name: String,
    ty: Type,
    affinity: CpuMask,
    prio: Priority,
    handles: RwLock<HandleMap>,
    signal: Mutex<Option<Signal>>,
}

unsafe impl Sync for TaskInfo {}

impl TaskInfo {
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn ty(&self) -> Type {
        self.ty
    }

    #[inline]
    pub fn affinity(&self) -> crate::cpu::CpuMask {
        self.affinity.clone()
    }

    #[inline]
    pub fn prio(&self) -> Priority {
        self.prio
    }

    #[inline]
    pub fn handles(&self) -> &RwLock<HandleMap> {
        &self.handles
    }

    /// # Safety
    ///
    /// This function must be called only if `PREEMPT` is locked.

    pub unsafe fn take_signal(&self) -> Option<Signal> {
        self.signal.lock().take()
    }

    pub fn replace_signal(&self, signal: Option<Signal>) -> Option<Signal> {
        let _pree = super::PREEMPT.lock();
        let mut self_signal = self.signal.lock();
        match (signal, &mut *self_signal) {
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

    pub fn update_signal<F, R>(&self, func: F) -> R
    where
        F: FnOnce(&mut Option<Signal>) -> R,
    {
        let _pree = super::PREEMPT.lock();
        func(&mut *self.signal.lock())
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

        let kstack = ctx::Kstack::new(entry, tid.ty);

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
    #[inline]
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

    #[inline]
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

    #[inline]
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
        idle::CTX_DROPPER.push(kstack);
    }

    #[inline]
    pub fn tid(&self) -> &Tid {
        &self.tid
    }

    #[inline]
    pub fn space(&self) -> &Space {
        &self.space
    }

    #[inline]
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
        KERNEL_GS.update_tss_rsp0(tss_rsp0);
        crate::mem::space::set_current(Arc::clone(&self.space));
        self.ext_frame.load();
        if !cpu::arch::in_intr() && self.tid.ty == Type::Kernel {
            KERNEL_GS.load();
        }
    }

    pub fn save_syscall_retval(&mut self, retval: usize) {
        debug_assert!(matches!(self.running_state, RunningState::Running(..)));

        self.kstack.task_frame_mut().set_syscall_retval(retval);
    }

    #[inline]
    pub fn kframe(&self) -> *mut u8 {
        self.kstack.kframe_ptr()
    }

    #[inline]
    pub fn kframe_mut(&mut self) -> *mut *mut u8 {
        self.kstack.kframe_ptr_mut()
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
    #[inline]
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
    #[inline]
    pub fn tid(&self) -> &Tid {
        &self.tid
    }

    #[inline]
    pub fn retval(&self) -> usize {
        self.retval
    }
}

#[inline]
pub(super) fn init() {
    CpuLocalLazy::force(&idle::IDLE);
}

fn create_common<F>(
    name: String,
    ty: Type,
    affinity: CpuMask,
    prio: Priority,
    dup_cur_space: bool,
    with_space: F,
    init_chan: Option<Channel>,
    arg: u64,
) -> Result<(Init, Handle)>
where
    F: FnOnce(&Arc<Space>) -> Result<(LAddr, Option<LAddr>, usize)>,
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

    let (entry, tls, stack_size) = with_space(&space)?;

    let (tid, init_handle, ret_wo) = {
        let ty = match ty {
            Type::Kernel => cur_tid.ty,
            Type::User => {
                if ty == Type::Kernel {
                    return Err(TaskError::Permission);
                } else {
                    Type::User
                }
            }
        };
        let prio = prio.min(cur_tid.prio);

        let mut new_ti = TaskInfo {
            from: UnsafeCell::new(None),
            name,
            ty,
            affinity,
            prio,
            handles: RwLock::new(HandleMap::new()),
            signal: Mutex::new(None),
        };
        let init_handle = init_chan.map(|chan| new_ti.handles.get_mut().insert(chan));
        let tid = tid::allocate(new_ti).map_err(|_| TaskError::TidExhausted)?;

        let (ret_wo, child) = {
            let child = Child::new(tid.clone());
            let _pree = PREEMPT.lock();
            (cur_tid.handles().write().insert(child.clone()), child)
        };

        unsafe { tid.from.get().write(Some((cur_tid, Some(child)))) };
        (tid, init_handle, ret_wo)
    };

    Init::new(
        tid,
        space,
        entry,
        stack_size,
        tls,
        [init_handle.map_or(0, |h| u64::from(h.raw())), arg],
    )
    .map(|task| (task, ret_wo))
}

pub fn create_fn(
    name: Option<String>,
    stack_size: usize,
    init_chan: Option<Channel>,
    func: LAddr,
    arg: *mut u8,
) -> Result<(Init, Handle)> {
    let (name, ty, affinity, prio) = super::SCHED
        .with_current(|cur| {
            (
                name.unwrap_or(format!("{}.func{:?}", cur.tid.name, *func)),
                cur.tid.ty,
                cur.tid.affinity.clone(),
                cur.tid.prio,
            )
        })
        .ok_or(TaskError::NoCurrentTask)?;

    create_common(
        name,
        ty,
        affinity,
        prio,
        true,
        |_| Ok((func, None, stack_size)),
        init_chan,
        arg as u64,
    )
}

pub(super) fn destroy(task: Dead) {
    tid::deallocate(&task.tid);
    if let Some((_, Some(child))) = { unsafe { &*task.tid.from.get() }.clone() } {
        let _ = child.cell().replace(task.retval);
    }
}
