pub mod ctx;
mod elf;
mod excep;
mod hdl;
pub mod idle;
pub mod prio;
pub mod sig;
mod sm;
mod syscall;
pub mod tid;

use alloc::{format, string::String, sync::Arc};

use paging::LAddr;
use solvent::Handle;
use spin::Lazy;

#[cfg(target_arch = "x86_64")]
pub use self::ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
use self::sig::Signal;
pub use self::{
    elf::from_elf, excep::dispatch_exception, hdl::HandleMap, prio::Priority, sm::*, tid::Tid,
};
use super::{ipc::Channel, PREEMPT};
use crate::{
    cpu::{self, CpuLocalLazy, CpuMask},
    mem::space::{Space, SpaceError},
};

static ROOT: Lazy<Tid> = Lazy::new(|| {
    let ti = TaskInfo::builder()
        .from(None)
        .name(String::from("ROOT"))
        .ty(Type::Kernel)
        .affinity(cpu::all_mask())
        .prio(prio::DEFAULT)
        .build()
        .unwrap();

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
        match self {
            TaskError::Permission => Error(EPERM),
            TaskError::NotSupported(_) => Error(EPERM),
            TaskError::InvalidFormat => Error(EINVAL),
            TaskError::Memory(err) => err.into(),
            TaskError::NoCurrentTask => Error(ESRCH),
            TaskError::TidExhausted => Error(EFAULT),
            TaskError::StackError(err) => err.into(),
            TaskError::Other(_) => Error(EFAULT),
        }
    }
}

pub type Result<T> = core::result::Result<T, TaskError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Kernel,
    User,
}

impl Type {
    #[inline]
    pub fn pass(this: Option<Self>, cur_ty: Type) -> Result<Type> {
        match (this, cur_ty) {
            (Some(Self::Kernel), Self::User) => Err(TaskError::Permission),
            (Some(ty), _) => Ok(ty),
            _ => Ok(cur_ty),
        }
    }
}

#[inline]
pub(super) fn init() {
    CpuLocalLazy::force(&idle::IDLE);
}

fn create_inner(
    cur: Tid,
    name: Option<String>,
    ty: Option<Type>,
    affinity: Option<CpuMask>,
    prio: Option<Priority>,
    space: Arc<Space>,
    entry: LAddr,
    init_chan: Option<Channel>,
    arg: u64,
    stack_size: usize,
) -> Result<(Init, Handle)> {
    let ty = Type::pass(ty, cur.ty())?;
    let ti = TaskInfo::builder()
        .from(Some(cur.clone()))
        .name(name.unwrap_or(format!("{}.func{}", cur.name(), archop::rand::get())))
        .ty(ty)
        .affinity(affinity.unwrap_or_else(|| cur.affinity()))
        .prio(prio.unwrap_or_else(|| cur.prio()))
        .build()
        .unwrap();

    let init_chan = init_chan.map(|chan| ti.handles().insert(chan).raw() as u64);
    let tid = tid::allocate(ti).map_err(|_| TaskError::TidExhausted)?;

    let entry = create_entry(&space, entry, stack_size, [init_chan.unwrap_or(0), arg])?;
    let kstack = ctx::Kstack::new(entry, ty);

    let ext_frame = ctx::ExtFrame::zeroed();

    let handle = cur.handles().insert(tid.clone());

    let init = Init::new(tid, space, kstack, ext_frame);

    Ok((init, handle))
}

pub fn create_fn(
    name: Option<String>,
    ty: Option<Type>,
    affinity: Option<CpuMask>,
    prio: Option<Priority>,
    func: LAddr,
    init_chan: Option<Channel>,
    arg: u64,
    stack_size: usize,
) -> Result<(Init, Handle)> {
    let (cur, space) = super::SCHED
        .with_current(|cur| {
            Type::pass(ty, cur.tid.ty()).map(|ty| (cur.tid.clone(), Space::clone(&cur.space, ty)))
        })
        .ok_or(TaskError::NoCurrentTask)??;

    create_inner(
        cur, name, ty, affinity, prio, space, func, init_chan, arg, stack_size,
    )
}
