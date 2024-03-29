use core::mem::size_of;

use archop::{msr, reg};
use paging::LAddr;

use super::seg::ndt::{INTR_CODE, USR_CODE_X86};
use crate::sched::task::ctx::arch::Frame;

extern "C" {
    fn rout_syscall();
}

/// # Safety
///
/// This function should only be called once per CPU.
pub unsafe fn init() -> sv_call::Result<LAddr> {
    let stack = crate::mem::alloc_system_stack()?
        .as_ptr()
        .sub(size_of::<usize>());

    let star = (USR_CODE_X86.into_val() as u64) << 48 | (INTR_CODE.into_val() as u64) << 32;
    msr::write(msr::STAR, star);
    msr::write(msr::LSTAR, rout_syscall as usize as u64);
    msr::write(msr::FMASK, reg::rflags::IF | reg::rflags::TF);

    let efer = msr::read(msr::EFER);
    msr::write(msr::EFER, efer | 1);

    Ok(LAddr::new(stack))
}

#[no_mangle]
unsafe extern "C" fn hdl_syscall(frame: *const Frame) {
    let syscall = (*frame).syscall_args();

    archop::resume_intr(None);
    let res = crate::syscall::handle(syscall);
    archop::pause_intr();

    let _ = crate::sched::SCHED.with_current(|cur| {
        cur.kstack_mut().task_frame_mut().set_syscall_retval(res);
        Ok(())
    });
}
