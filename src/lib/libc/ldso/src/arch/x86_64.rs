use core::arch::asm;

pub unsafe fn get_tls_reg() -> u64 {
    let mut ret;
    asm!("rdfsbase {}", out(reg) ret, options(nostack));
    ret
}

pub unsafe fn set_tls_reg(value: u64) {
    asm!("wrfsbase {}", in(reg) value, options(nostack));
}
