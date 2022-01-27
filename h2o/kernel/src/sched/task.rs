pub mod ctx;
mod elf;
mod excep;
pub mod hdl;
mod idle;
mod sig;
mod sm;
mod syscall;
mod tid;

use alloc::{format, string::String, sync::Arc};
use core::any::Any;

use paging::LAddr;
use solvent::Handle;

#[cfg(target_arch = "x86_64")]
pub use self::ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
use self::elf::from_elf;
pub use self::{excep::dispatch_exception, sig::Signal, sm::*, tid::Tid};
use super::{ipc::Channel, PREEMPT};
use crate::{
    cpu::{CpuLocalLazy, CpuMask},
    mem::space::Space,
};

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
    pub fn pass(this: Option<Self>, cur_ty: Type) -> solvent::Result<Type> {
        match (this, cur_ty) {
            (Some(Self::Kernel), Self::User) => Err(solvent::Error::EPERM),
            (Some(ty), _) => Ok(ty),
            _ => Ok(cur_ty),
        }
    }
}

#[inline(never)]
pub(super) fn init() {
    CpuLocalLazy::force(&idle::CTX_DROPPER);
    CpuLocalLazy::force(&idle::IDLE);
}

#[inline]
pub fn init_early() {
    hdl::init();
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
    init_chan: hdl::Ref<dyn Any>,
    s: &Starter,
) -> solvent::Result<(Init, Handle)> {
    let ty = Type::pass(ty, cur.ty())?;
    let ti = TaskInfo::builder()
        .from(Some(cur.clone()))
        .name(name.unwrap_or(format!("{}.func{}", cur.name(), archop::rand::get())))
        .ty(ty)
        .affinity(affinity.unwrap_or_else(|| cur.affinity()))
        .build()
        .unwrap();

    let init_chan = unsafe { ti.handles().insert_ref(init_chan) }?;

    let tid = tid::allocate(ti).map_err(|_| solvent::Error::EBUSY)?;

    let entry = create_entry(s.entry, s.stack, [init_chan.raw() as u64, s.arg]);
    let kstack = ctx::Kstack::new(Some(entry), ty);
    let ext_frame = ctx::ExtFrame::zeroed();

    let handle = cur.handles().insert(tid.clone())?;

    let init = Init::new(tid, space, kstack, ext_frame);

    Ok((init, handle))
}

#[inline]
fn exec(
    name: Option<String>,
    space: Arc<Space>,
    init_chan: hdl::Ref<dyn Any>,
    starter: &Starter,
) -> solvent::Result<(Init, Handle)> {
    let cur = super::SCHED.with_current(|cur| Ok(cur.tid.clone()))?;
    exec_inner(cur, name, None, None, space, init_chan, starter)
}

#[inline]
fn create(name: Option<String>, space: Arc<Space>) -> solvent::Result<(Init, solvent::Handle)> {
    let cur = super::SCHED.with_current(|cur| Ok(cur.tid.clone()))?;

    let ty = cur.ty();
    let ti = TaskInfo::builder()
        .from(Some(cur.clone()))
        .name(name.unwrap_or(format!("{}.func{}", cur.name(), archop::rand::get())))
        .ty(ty)
        .affinity(cur.affinity())
        .build()
        .unwrap();

    let tid = tid::allocate(ti).map_err(|_| solvent::Error::EBUSY)?;

    let kstack = ctx::Kstack::new(None, ty);
    let ext_frame = ctx::ExtFrame::zeroed();

    let handle = cur.handles().insert(tid.clone())?;

    let init = Init::new(tid, space, kstack, ext_frame);

    Ok((init, handle))
}
