//! # Segmentation in x86_64.
//!
//! This module deals with segmentation, including so-called descriptors and descriptor
//! tables. This module is placed in `cpu`, not `mem`, mainly in that segmentation nowadays
//! is generally not a method to manage memory, but an aspect in configuring CPU and its
//! surroundings.

pub mod idt;
pub mod ndt;

use paging::{LAddr, PAddr};

use core::mem::{size_of, transmute};
use core::ops::Range;
use modular_bitfield::prelude::*;
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
/// If `rpl` and `ti` is masked off, the structure as `u16` is the offset of the target
/// descriptor in the table.
#[bitfield]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegSelector {
      /// The Request Privilege Level.
      pub rpl: B2,
      /// The Table Index - 0 for GDT and 1 for LDT.
      pub ti: bool,
      /// The numeric index of the descriptor table.
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

/// Get the type of a segment or gate descriptor.
///
/// We can do this because the offset of the type attribute of 3 kinds of descriptors is
/// the same.
///
/// # Safety
///
/// The caller must ensure the validity of `ptr` as a segment or gate descriptor.
pub unsafe fn get_type_attr(ptr: *mut u8) -> u16 {
      let ptr = ptr.add(size_of::<u32>() + size_of::<u8>());
      (ptr as *mut u16).read()
}

/// # Safety
///
/// The caller must ensure the value stored in [`archop::msr::FS_BASE`] is a
/// valid physical address.
pub(super) unsafe fn reload_pls() {
      use archop::msr;

      let val = msr::read(msr::FS_BASE) as usize;
      if val != 0 {
            let ptr = PAddr::new(val).to_laddr(minfo::ID_OFFSET).cast::<usize>();

            ptr.write(ptr as usize);

            msr::write(msr::FS_BASE, ptr as u64);
      }
}
