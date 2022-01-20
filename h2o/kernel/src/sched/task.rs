pub mod ctx;
mod elf;
mod excep;
pub mod hdl;
pub mod idle;
pub mod sig;
mod sm;
mod syscall;
pub mod tid;

use alloc::{format, string::String, sync::Arc};
use core::any::Any;

use paging::LAddr;
use solvent::Handle;

#[cfg(target_arch = "x86_64")]
pub use self::ctx::arch::{DEFAULT_STACK_LAYOUT, DEFAULT_STACK_SIZE};
pub use self::{elf::from_elf, excep::dispatch_exception, sm::*, tid::Tid};
use self::{hdl::Ref, sig::Signal};
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

#[inline]
pub(super) fn init() {
    CpuLocalLazy::force(&idle::IDLE);
}

fn create_inner(
    cur: Tid,
    name: Option<String>,
    ty: Option<Type>,
    affinity: Option<CpuMask>,
    space: Arc<Space>,
    entry: LAddr,
    init_chan: hdl::Ref<dyn Any>,
    arg: u64,
    stack_size: usize,
) -> solvent::Result<(Init, Handle)> {
    let ty = Type::pass(ty, cur.ty())?;
    let ti = TaskInfo::builder()
        .from(Some(cur.clone()))
        .name(name.unwrap_or(format!("{}.func{}", cur.name(), archop::rand::get())))
        .ty(ty)
        .affinity(affinity.unwrap_or_else(|| cur.affinity()))
        .build()
        .unwrap();

    let init_chan = unsafe { ti.handles().insert_ref(init_chan) }.unwrap();

    let (stack_virt, stack) = space.init_stack(stack_size)?;
    unsafe { ti.handles().insert_unchecked(stack_virt, false, false) }?;

    let tid = tid::allocate(ti).map_err(|_| solvent::Error::EBUSY)?;
    let entry = create_entry(entry, stack, [init_chan.raw() as u64, arg]);
    let kstack = ctx::Kstack::new(entry, ty);

    let ext_frame = ctx::ExtFrame::zeroed();

    let handle = cur.handles().insert(tid.clone()).unwrap();

    let init = Init::new(tid, space, kstack, ext_frame);

    Ok((init, handle))
}

pub fn create_fn(
    name: Option<String>,
    ty: Option<Type>,
    affinity: Option<CpuMask>,
    func: LAddr,
    init_chan: hdl::Ref<dyn Any>,
    arg: u64,
    stack_size: usize,
) -> solvent::Result<(Init, Handle)> {
    let (cur, space) =
        super::SCHED.with_current(|cur| Ok((cur.tid.clone(), Arc::clone(&cur.space))))?;

    create_inner(
        cur, name, ty, affinity, space, func, init_chan, arg, stack_size,
    )
}
