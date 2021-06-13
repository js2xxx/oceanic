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

impl Frame {
      const RFLAGS: &'static str =
            "CF - PF - AF - ZF SF TF IF DF OF IOPLL IOPLH NT - RF VM AC VIF VIP ID";

      pub const ERRC: &'static str = "EXT IDT TI";
      pub const ERRC_PF: &'static str = "P WR US RSVD ID PK SS - - - - - - - - SGX";

      pub fn dump(&self, errc_format: &'static str) {
            use crate::log::flags::Flags;
            use log::info;

            info!("Frame dump");

            if self.errc != 0u64.wrapping_sub(1) {
                  info!("> Error Code = {}", Flags::new(self.errc, errc_format));
            }
            info!("> Address = {:#018X}", self.rip);
            info!("> RFlags  = {}", Flags::new(self.rflags, Self::RFLAGS));

            info!("> GPRs: ");
            info!("  rax = {:#018X}, rcx = {:#018X}", self.rax, self.rcx);
            info!("  rdx = {:#018X}, rbx = {:#018X}", self.rdx, self.rbx);
            info!("  rbp = {:#018X}, rsp = {:#018X}", self.rbp, self.rsp);
            info!("  rsi = {:#018X}, rdi = {:#018X}", self.rsi, self.rdi);
            info!("  r8  = {:#018X}, r9  = {:#018X}", self.r8, self.r9);
            info!("  r10 = {:#018X}, r11 = {:#018X}", self.r10, self.r11);
            info!("  r12 = {:#018X}, r13 = {:#018X}", self.r12, self.r13);
            info!("  r14 = {:#018X}, r15 = {:#018X}", self.r14, self.r15);

            info!("> Segments:");
            info!("  cs  = {:#018X}, ss  = {:#018X}", self.cs, self.ss);
      }
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
