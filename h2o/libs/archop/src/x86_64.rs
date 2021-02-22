pub mod io;
pub mod msr;
pub mod reg;

use paging::LAddr;

use core::ops::Range;

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
/// Invalid use of this function can cause CPU unrecoverable fault.
#[inline]
pub unsafe fn halt() {
      asm!("hlt");
}

/// # Safety
///
/// Invalid use of this function can cause CPU unrecoverable fault.
#[inline]
pub unsafe fn pause_intr() {
      asm!("cli");
}

/// # Safety
///
/// Invalid use of this function can cause CPU unrecoverable fault.
#[inline]
pub unsafe fn resume_intr() {
      asm!("sti");
}

/// # Safety
///
/// Invalid use of this function can cause CPU unrecoverable fault.
#[inline(always)]
pub unsafe fn halt_loop(intr_op: Option<bool>) -> ! {
      let f = match intr_op {
            Some(op) => {
                  if op {
                        resume_intr
                  } else {
                        pause_intr
                  }
            }
            None => || {},
      };
      loop {
            f();
            halt();
      }
}

/// # Safety
///
/// Invalid use of this function can cause CPU unrecoverable fault.
#[inline]
pub unsafe fn pause() {
      asm!("pause");
}
