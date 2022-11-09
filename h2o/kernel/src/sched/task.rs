mod boot;
pub mod ctx;
mod elf;
mod excep;
pub mod hdl;
mod idle;
mod sig;
mod sm;
mod space;
mod syscall;
mod tid;

use alloc::{format, string::String, sync::Arc};

use paging::LAddr;

#[cfg(target_arch = "x86_64")]
pub use self::ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
use self::elf::from_elf;
pub use self::{boot::VDSO, excep::dispatch_exception, sig::Signal, sm::*, space::Space, tid::Tid};
use super::{ipc::Channel, Arsc, PREEMPT};
use crate::cpu::{CpuMask, Lazy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Kernel,
    User,
}

impl Type {
    /// # Errors
    ///
    /// Returns error if current task's type is less privileged than the
    /// expected type.
    #[inline]
    pub fn pass(this: Option<Self>, cur_ty: Type) -> sv_call::Result<Type> {
        match (this, cur_ty) {
            (Some(Self::Kernel), Self::User) => Err(sv_call::EPERM),
            (Some(ty), _) => Ok(ty),
            _ => Ok(cur_ty),
        }
    }
}

#[inline(never)]
pub(super) fn init() {
    Lazy::force(&idle::CTX_DROPPER);
    Lazy::force(&idle::IDLE);
}

#[inline]
pub fn init_early() {
    tid::init();
}

#[derive(Debug, Clone, Copy)]
struct Starter {
    entry: LAddr,
    stack: LAddr,
    arg: u64,
}

fn exec_inner(
    cur: Tid,
    name: Option<String>,
    ty: Option<Type>,
    affinity: Option<CpuMask>,
    space: Arc<Space>,
    init_chan: sv_call::Handle,
    s: &Starter,
) -> sv_call::Result<Init> {
    let ty = Type::pass(ty, cur.ty())?;
    let ti = TaskInfo::builder()
        .from(cur.downgrade())
        .excep_chan(Arsc::try_new(Default::default())?)
        .name(name.unwrap_or(format!("{}.func{}", cur.name(), archop::rand::get())))
        .ty(ty)
        .affinity(affinity.unwrap_or_else(|| cur.affinity()))
        .build()
        .unwrap();

    let tid = tid::allocate(ti).map_err(|_| sv_call::EBUSY)?;
    space.set_main(&tid);

    let entry = ctx::Entry {
        entry: s.entry,
        stack: s.stack,
        args: [init_chan.raw() as u64, s.arg],
    };
    let kstack = ctx::Kstack::new(Some(entry), ty);
    let ext_frame = ctx::ExtFrame::zeroed();

    let init = Init::new(tid, space, kstack, ext_frame);

    Ok(init)
}

#[inline]
fn exec(
    name: Option<String>,
    space: Arc<Space>,
    init_chan: sv_call::Handle,
    starter: &Starter,
) -> sv_call::Result<(Init, sv_call::Handle)> {
    let cur = super::SCHED.with_current(|cur| Ok(cur.tid().clone()))?;
    let init = exec_inner(cur, name, None, None, space, init_chan, starter)?;
    super::SCHED.with_current(|cur| {
        let event = Arc::downgrade(&init.tid().event) as _;
        let handle = cur
            .space()
            .handles()
            .insert(init.tid().clone(), Some(event))?;
        Ok((init, handle))
    })
}

fn create(
    name: Option<String>,
    space: Arc<Space>,
    init_chan: sv_call::Handle,
) -> sv_call::Result<(Init, sv_call::Handle)> {
    let cur = super::SCHED.with_current(|cur| Ok(cur.tid().clone()))?;

    let ty = cur.ty();
    let ti = TaskInfo::builder()
        .from(cur.downgrade())
        .excep_chan(Arsc::try_new(Default::default())?)
        .name(name.unwrap_or(format!("{}.func{}", cur.name(), archop::rand::get())))
        .ty(ty)
        .affinity(cur.affinity())
        .build()
        .unwrap();

    let tid = tid::allocate(ti).map_err(|_| sv_call::EBUSY)?;
    space.set_main(&tid);

    let mut kstack = ctx::Kstack::new(None, ty);
    kstack.task_frame_mut().set_args(init_chan.raw() as _, 0);
    let ext_frame = ctx::ExtFrame::zeroed();

    let init = Init::new(tid, space, kstack, ext_frame);

    super::SCHED.with_current(|cur| {
        let event = Arc::downgrade(&init.tid().event) as _;
        let handle = cur
            .space()
            .handles()
            .insert(init.tid().clone(), Some(event))?;
        Ok((init, handle))
    })
}
