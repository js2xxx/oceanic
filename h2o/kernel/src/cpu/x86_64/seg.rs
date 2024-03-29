//! # Segmentation in x86_64.
//!
//! This module deals with segmentation, including so-called descriptors and
//! descriptor tables. This module is placed in `cpu`, not `mem`, mainly in that
//! segmentation nowadays is generally not a method to manage memory, but an
//! aspect in configuring CPU and its surroundings.

pub mod idt;
pub mod ndt;

use alloc::alloc::Global;
use core::{
    alloc::{Allocator, Layout},
    mem::{size_of, transmute},
    ops::Range,
    ptr::NonNull,
};

use modular_bitfield::prelude::*;
use paging::{LAddr, PAddr};
use static_assertions::*;

/// The all available rings - privilege levels.
pub const PL: Range<u16> = 0..4;

/// The all available limit ranges.
pub const LIMIT: Range<u32> = 0..0x100000;

/// The all available interrupt stack tables.
pub const IST: Range<u8> = 1..8;

/// The attributes for segment and gate descriptors.
pub mod attrs {
    pub const SEG_CODE: u16 = 0x1A;
    pub const SEG_DATA: u16 = 0x12;
    pub const SYS_TSS: u16 = 0x09;
    pub const SYS_LDT: u16 = 0x02;
    pub const INT_GATE: u16 = 0x0E;
    #[allow(dead_code)]
    pub const TRAP_GATE: u16 = 0x0F;
    pub const PRESENT: u16 = 0x80;
    pub const X86: u16 = 0x4000;
    pub const X64: u16 = 0x2000;
    pub const G4K: u16 = 0x8000;
}

/// The errors happening when setting a descriptor.
#[derive(Debug)]
pub enum SetError {
    /// Indicate field is out of its available range.
    OutOfRange,
}

/// The pointer used to load descriptor tables.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct FatPointer {
    /// The value of length - 1.
    pub limit: u16,
    pub base: LAddr,
}

/// The index of descriptor tables.
///
/// If `rpl` and `ti` is masked off, the structure as `u16` is the offset of the
/// target descriptor in the table.
#[bitfield]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegSelector {
    /// The Request Privilege Level.
    #[allow(clippy::return_self_not_must_use)]
    pub rpl: B2,
    /// The Table Index - 0 for GDT and 1 for LDT.
    #[allow(clippy::return_self_not_must_use)]
    pub ti: bool,
    /// The numeric index of the descriptor table.
    #[allow(clippy::return_self_not_must_use)]
    pub index: B13,
}
const_assert_eq!(size_of::<SegSelector>(), size_of::<u16>());

impl SegSelector {
    pub const fn from_const(data: u16) -> Self {
        unsafe { transmute(data) }
    }

    pub fn into_val(self) -> u16 {
        unsafe { transmute(self) }
    }
}

impl Default for SegSelector {
    fn default() -> Self {
        Self::from(0)
    }
}

impl From<u16> for SegSelector {
    fn from(data: u16) -> Self {
        Self::from_const(data)
    }
}

/// Reload the Processor-Local Storage of the bootstrap CPU.
///
/// # Safety
///
/// The caller must ensure the value stored in FS base is a valid physical
/// address.
pub unsafe fn reload_pls() {
    extern "C" {
        static TDATA_START: u8;
        static TBSS_START: u8;
    }
    use archop::reg;
    let pls_size = crate::kargs().pls_layout.map_or(0, |layout| layout.size());

    let val = reg::read_fs() as usize;
    if val != 0 {
        let ptr = PAddr::new(val).to_laddr(minfo::ID_OFFSET).cast::<usize>();
        let base = ptr.cast::<u8>().sub(pls_size);
        let size = (&TBSS_START as *const u8).offset_from(&TDATA_START) as usize;
        base.copy_from_nonoverlapping(&TDATA_START, size);
        base.add(size).write_bytes(0, pls_size - size);
        ptr.write(ptr as usize);

        reg::write_fs(ptr as u64);
    }

    test_pls();
}

/// Allocate and initialize a new PLS for application CPU.
pub fn alloc_pls() -> sv_call::Result<NonNull<u8>> {
    extern "C" {
        static TDATA_START: u8;
        static TBSS_START: u8;
    }

    let pls_layout = match crate::kargs().pls_layout {
        Some(layout) => layout,
        None => return Err(sv_call::ENOENT),
    };

    let base = Global
        .allocate_zeroed(
            pls_layout
                .extend(Layout::new::<*mut u8>())
                .expect("Failed to get the allocation layout")
                .0,
        )
        .map(NonNull::as_non_null_ptr)?;
    unsafe {
        let size = (&TBSS_START as *const u8).offset_from(&TDATA_START) as usize;
        base.as_ptr().copy_from_nonoverlapping(&TDATA_START, size);

        let self_ptr = base.as_ptr().add(pls_layout.size());
        self_ptr.cast::<*mut u8>().write(self_ptr);

        Ok(NonNull::new_unchecked(self_ptr))
    }
}

#[inline]
pub fn test_pls() {
    #[cfg(debug_assertions)]
    {
        #[thread_local]
        static mut A: usize = 0;
        #[thread_local]
        static mut B: usize = 5;
        unsafe {
            debug_assert_eq!(A, 0);
            A += 1;
            debug_assert_eq!(A, 1);
            debug_assert_eq!(B, 5);
            B -= 1;
            debug_assert_eq!(B, 4);
        }
    }
}

/// Initialize segmentation structures
///
/// # Safety
///
/// The caller must ensure that this function is called only once from the
/// bootstrap CPU.
#[inline]
pub(super) unsafe fn init() {
    ndt::init();
    idt::init();
}

#[inline]
pub(super) unsafe fn init_ap() {
    ndt::init();
    idt::init();
}
