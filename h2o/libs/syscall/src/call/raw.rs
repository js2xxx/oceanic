/// # Safety
///
/// The caller is responsible for the arguments and the results of the syscall.
#[inline]
pub unsafe fn syscall(
    num: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> usize {
    let mut ret = num;
    core::arch::asm!(
          "syscall",
          inout("rax") ret,
          in("rdi") arg1,
          in("rsi") arg2,
          in("rdx") arg3,
          in("r8") arg4,
          in("r9") arg5,
          out("rcx") _,
          out("r11") _,
          options(nostack)
    );
    ret
}

#[inline]
pub fn pack_syscall(
    num: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    arg5: usize,
) -> crate::Syscall {
    crate::Syscall {
        num,
        args: [arg1, arg2, arg3, arg4, arg5],
    }
}
