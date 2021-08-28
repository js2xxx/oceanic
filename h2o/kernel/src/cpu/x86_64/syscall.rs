use super::seg::ndt::{INTR_CODE, USR_CODE_X86};
use crate::sched::task::ctx::arch::Frame;
use archop::{msr, reg};
use paging::LAddr;

extern "C" {
      fn rout_syscall();
}

/// # Safety
///
/// This function should only be called once per CPU.
pub unsafe fn init() -> Option<LAddr> {
      let stack = {
            let layout = crate::sched::task::DEFAULT_STACK_LAYOUT;
            let base = alloc::alloc::alloc(layout);
            if base.is_null() {
                  return None;
            }
            base.add(layout.size())
      };

      let rflags = reg::rflags::IF & reg::rflags::DF & reg::rflags::TF;
      msr::write(msr::FMASK, rflags);

      msr::write(msr::LSTAR, rout_syscall as u64);

      let star = (USR_CODE_X86.into_val() as u64) << 48 | (INTR_CODE.into_val() as u64) << 32;
      msr::write(msr::STAR, star);

      let efer = msr::read(msr::EFER);
      msr::write(msr::EFER, efer | 1);

      Some(LAddr::new(stack))
}

#[no_mangle]
unsafe extern "C" fn hdl_syscall(frame: *const Frame) {
      let arg = (*frame).syscall_args();
      let res = crate::syscall::handler(&arg);
      if !matches!(res, Err(solvent::Error(0))) {
            let val = solvent::Error::encode(res);
            let mut sched = crate::sched::SCHED.lock();
            if let Some(cur) = sched.current_mut() {
                  cur.save_syscall_retval(val);
            }
      }
}
