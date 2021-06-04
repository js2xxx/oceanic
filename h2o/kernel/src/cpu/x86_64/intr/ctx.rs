#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Frame {
      r15: u64,
      r14: u64,
      r13: u64,
      r12: u64,
      r11: u64,
      r10: u64,
      r9: u64,
      r8: u64,
      rsi: u64,
      rdi: u64,
      rbp: u64,
      rbx: u64,
      rdx: u64,
      rcx: u64,
      rax: u64,

      pub errc: u64,

      pub rip: u64,
      pub cs: u64,
      pub rflags: u64,
      pub rsp: u64,
      pub ss: u64,
}

/// A temporary module for storing the thread stack.
/// Must be removed after thread module creation.
pub mod test {
      use super::Frame;
      static mut THREAD_STACK_TOP: *mut u8 = core::ptr::null_mut();

      pub unsafe fn save_regs(frame: *const Frame) -> *mut u8 {
            let thread_frame = THREAD_STACK_TOP.cast::<Frame>().sub(1);
            thread_frame.copy_from(frame, 1);
            thread_frame.cast()
      }

      pub unsafe fn init_stack_top(st: *mut u8) {
            THREAD_STACK_TOP = st;
      }
}