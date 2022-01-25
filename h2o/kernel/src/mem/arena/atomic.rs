use core::{arch::global_asm, cell::UnsafeCell, mem};

use static_assertions::const_assert_eq;

const_assert_eq!(mem::size_of::<(u64, u64)>(), 16);

// rdi, rsi, rdx, rcx
global_asm!(
    "
cas128_raw:
    push rbx

    mov  rbx, rdx
    mov  rax, [rsi]
    mov  rdx, [rsi + 8]
    lock
    cmpxchg16b [rdi]
    setz r8b

    mov  [rsi], rax
    mov  [rsi + 8], rdx
    mov  al, r8b

    pop  rbx
    ret
    "
);
extern "C" {
    fn cas128_raw(mem: *mut u8, old: *mut u8, new_hi: u64, new_lo: u64) -> bool;
}

#[inline]
unsafe fn cas128(mem: *mut (u64, u64), old: (u64, u64), new: (u64, u64)) -> ((u64, u64), bool) {
    let mut ret = old;
    let flag = cas128_raw(
        mem.cast(),
        (&mut ret as *mut (u64, u64)).cast(),
        new.0,
        new.1,
    );
    (ret, flag)
}

#[derive(Debug, Default)]
#[repr(align(16))]
pub struct AtomicDoubleU64 {
    inner: UnsafeCell<(u64, u64)>,
}

unsafe impl Send for AtomicDoubleU64 {}
unsafe impl Sync for AtomicDoubleU64 {}

impl AtomicDoubleU64 {
    pub fn compare_exchange_acqrel(
        &self,
        old: (u64, u64),
        new: (u64, u64),
    ) -> Result<(u64, u64), (u64, u64)> {
        let (ret, flag) = unsafe { cas128(self.inner.get().cast(), old, new) };
        if flag {
            Ok(ret)
        } else {
            Err(ret)
        }
    }

    #[inline]
    pub fn load_acquire(&self) -> (u64, u64) {
        self.compare_exchange_acqrel((0, 0), (0, 0))
            .into_ok_or_err()
    }
}
