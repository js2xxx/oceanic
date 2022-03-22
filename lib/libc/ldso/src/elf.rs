use core::mem;

pub use goblin::elf64::{
    dynamic::*, gnu_hash::*, header::*, program_header::*, reloc::*, section_header::*, sym::*,
    Note,
};

pub const PT_NUM: u32 = 10;

pub const DT_RELR: u64 = 36;
pub const DT_RELRSZ: u64 = 35;
pub const DT_RELRENT: u64 = 37;

pub const DT_NUM: u64 = 38;

/// # Safety
///
/// `base` must contains a valid reference to a statically mapped ELF structure
/// and `relr` must be the RELR entry in its dynamic section.
#[inline(always)]
pub unsafe fn apply_relr(base: *mut u8, relr: *const usize, size: usize) {
    let len = size / mem::size_of::<usize>();

    let mut i = 0;
    while i < len {
        let addr = base.add(*relr.add(i)).cast::<usize>();
        i += 1;

        *addr += base as usize;

        let mut addr = addr.add(1);
        while i < len && *relr.add(i) & 1 != 0 {
            let mut run = addr;
            addr = addr.add(usize::BITS as usize - 1);

            let mut bitmask = *relr.add(i) >> 1;
            i += 1;
            while bitmask != 0 {
                let skip = bitmask.trailing_zeros() as usize;
                run = run.add(skip);
                *run += base as usize;
                run = run.add(1);
                bitmask >>= skip + 1;
            }
        }
    }
}
