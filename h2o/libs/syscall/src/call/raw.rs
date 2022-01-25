use crate::call::reg::SerdeReg;

/// # Safety
///
/// The caller is responsible for the arguments and the results of the syscall.
pub unsafe fn syscall(arg: &crate::Arguments) -> crate::Result<usize> {
    let crate::Arguments {
        fn_num: mut rax,
        args: [rdi, rsi, rdx, r8, r9],
    } = *arg;
    core::arch::asm!(
          "syscall",
          inout("rax") rax,
          in("rdi") rdi,
          in("rsi") rsi,
          in("rdx") rdx,
          in("r8") r8,
          in("r9") r9,
          out("rcx") _,
          out("r11") _,
          options(nostack)
    );
    crate::Result::decode(rax)
}
