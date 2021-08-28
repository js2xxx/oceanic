use super::seg::ndt::{INTR_CODE, USR_CODE_X86};
use super::seg::SegSelector;
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

      let rflags = (reg::rflags::read() & 0xFFFFFFFF)
            & !reg::rflags::IF
            & !reg::rflags::DF
            & !reg::rflags::TF;
      msr::write(msr::FMASK, rflags);

      msr::write(msr::LSTAR, rout_syscall as u64);

      let star = (SegSelector::into_val(USR_CODE_X86) as u64) << 48
            | (SegSelector::into_val(INTR_CODE) as u64) << 32;
      msr::write(msr::STAR, star);

      let efer = msr::read(msr::EFER);
      msr::write(msr::EFER, efer | 1);

      Some(LAddr::new(stack))
}

#[no_mangle]
unsafe extern "C" fn hdl_syscall(frame: *const Frame) -> *const Frame {
      archop::pause();
      frame
}
