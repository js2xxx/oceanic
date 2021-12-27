pub mod fpu;
pub mod io;
mod lock;
pub mod msr;
pub mod rand;
pub mod reg;

use core::{arch::asm, ops::Range};

use bitop_ex::BitOpEx;
pub use lock::{mutex::*, rwlock::*, IntrState, PreemptState, PreemptStateGuard};
use paging::LAddr;

/// The address space that should never be valid due to hardware constraints.
pub const INCANONICAL: Range<LAddr> =
    LAddr::new(0x8000_0000_0000 as *mut u8)..LAddr::new(0xFFFF_8000_0000_0000 as *mut u8);

/// Check if the address is not permanently invalid.
pub fn canonical(addr: LAddr) -> bool {
    !INCANONICAL.contains(&addr)
}

/// Automatically fix an incanonical address to its most likely target location.
///
/// NOTE: It should not be misused to hard-correct addresses
pub fn fix_canonical(addr: LAddr) -> LAddr {
    let ret = LAddr::from(addr.val() & 0xFFFF_FFFF_FFFF);
    if canonical(ret) {
        ret
    } else {
        LAddr::new(unsafe { ret.add(0xFFFF_0000_0000_0000) })
    }
}

/// # Safety
///
/// Invalid use of this function can cause CPU's unrecoverable fault.
#[inline]
pub unsafe fn halt() {
    asm!("hlt");
}

/// # Safety
///
/// Invalid use of this function can cause CPU unrecoverable fault.
#[inline]
pub unsafe fn pause_intr() -> u64 {
    let rflags = reg::rflags::read();
    asm!("cli");
    rflags
}

/// # Safety
///
/// Invalid use of this function can cause CPU unrecoverable fault.
#[inline]
pub unsafe fn resume_intr(rflags: Option<u64>) {
    if rflags.map_or(true, |rflags| rflags.contains_bit(reg::rflags::IF)) {
        asm!("sti");
    }
}

/// # Safety
///
/// Invalid use of this function can cause CPU unrecoverable fault.
#[inline(always)]
pub unsafe fn halt_loop(intr_op: Option<bool>) -> ! {
    let f = match intr_op {
        Some(op) => {
            if op {
                || resume_intr(None)
            } else {
                || {
                    pause_intr();
                }
            }
        }
        None => || {},
    };
    loop {
        f();
        halt();
    }
}
