pub mod ctx;
pub mod elf;
pub mod hdl;
pub mod idle;
pub mod prio;
pub mod sig;
pub mod tid;

use alloc::{boxed::Box, format, string::String, sync::Arc};
use core::time::Duration;

use paging::LAddr;
use spin::Lazy;

#[cfg(target_arch = "x86_64")]
pub use self::ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
use self::sig::Signal;
pub use self::{
    elf::from_elf,
    hdl::{UserHandle, UserHandles},
    prio::Priority,
    tid::Tid,
};
use super::wait::{WaitCell, WaitObject};
use crate::{
    cpu::{arch::KernelGs, time::Instant, CpuMask},
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

    pub fn signal(&self) -> Option<Signal> {
        self.signal
    }

    pub fn set_signal(&mut self, signal: Option<Signal>) -> Option<Signal> {
        match (signal, &mut self.signal) {
            (None, s) => {
                *s = None;
                None
            }
            (Some(signal), s) if s.is_none() => {
                *s = Some(signal);
                None
            }
            (Some(signal), s) => (s.unwrap() >= signal).then(|| s.replace(signal).unwrap()),
        }
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

        let kstack = ctx::Kstack::new(entry, ti.ty);

        let tid = tid::alloc_insert_or(ti, |_ti| {
            let _ = space.clear_stack();
            TaskError::TidExhausted
        })?;

        Ok(Init { tid, space, kstack })
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

    pub(in crate::sched) fn block(this: Self, wo: &WaitObject, block_desc: &'static str) {
        let Ready {
            tid,
            space,
            kstack,
            ext_frame,
            cpu,
            runtime,
            ..
        } = this;
        let blocked = Blocked {
            tid,
            space,
            kstack,
            ext_frame,
            cpu,
            block_desc,
            runtime,
        };
        wo.wait_queue.push(blocked);
    }

    pub(in crate::sched) fn exit(this: Self, retval: usize) {
        let Ready { tid, kstack, .. } = this;
        let dead = Dead { tid, retval };
        destroy(dead);
        idle::CTX_DROPPER.push(kstack);
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
    pub unsafe fn save_intr(&mut self) {
        debug_assert!(!matches!(self.running_state, RunningState::NotRunning));

        self.ext_frame.save();
    }

    pub unsafe fn load_intr(&self) {
        debug_assert!(!matches!(self.running_state, RunningState::NotRunning));

        let tss_rsp0 = self.kstack.top().val() as u64;
        KernelGs::update_tss_rsp0(tss_rsp0);
        crate::mem::space::set_current(self.space.clone());
        self.ext_frame.load();
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
    kstack: Box<ctx::Kstack>,
    ext_frame: Box<ctx::ExtendedFrame>,

    cpu: usize,
    block_desc: &'static str,
    runtime: Duration,
}

// #[derive(Debug)]
// pub struct Killed {
//     tid: Tid,
// }

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
    args: [u64; 2],
) -> Result<(Init, UserHandle)>
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

        let ret_wo = cur_ti
            .user_handles
            .insert(WaitCell::<usize>::new())
            .unwrap();

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
                signal: None,
            },
            ret_wo,
        )
    };

    Init::new(ti, space, entry, stack_size, tls, args).map(|task| (task, ret_wo))
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
        [arg as u64, 0],
    )
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
        crate::sched::SCHED.exit_current(retval);

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
    pub fn task_join(hdl: u32) -> usize {
        use core::num::NonZeroU32;

        let wc_hdl = super::UserHandle::new(NonZeroU32::new(hdl).ok_or(Error(EINVAL))?);

        let cur_tid = crate::sched::SCHED
            .with_current(|cur| cur.tid)
            .ok_or(Error(ESRCH))?;

        let wc = {
            let ti = super::tid::get(&cur_tid).ok_or(Error(ESRCH))?;
            ti.user_handles
                .get::<alloc::sync::Arc<crate::sched::wait::WaitCell<usize>>>(wc_hdl)
                .ok_or(Error(ECHILD))?
                .clone()
        };

        Ok(wc.take("task_join"))
    }

    #[syscall]
    pub fn task_ctl(hdl: u32, op: u32) {
        match op {
            // Kill
            1 => {
                use core::num::NonZeroU32;

                let wc_hdl = super::UserHandle::new(NonZeroU32::new(hdl).ok_or(Error(EINVAL))?);

                crate::sched::SCHED
                    .with_current(|cur| {
                        super::tid::get_mut(&cur.tid)
                            .map(|mut ti| ti.set_signal(Some(super::Signal::Kill)))
                    })
                    .flatten()
                    .ok_or(Error(ESRCH))?;

                Ok(())
            }
            // Suspend
            2 => todo!(),
            _ => Err(Error(EINVAL)),
        }
    }
}
