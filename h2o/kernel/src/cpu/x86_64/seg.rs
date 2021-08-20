//! # Segmentation in x86_64.
//!
//! This module deals with segmentation, including so-called descriptors and descriptor
//! tables. This module is placed in `cpu`, not `mem`, mainly in that segmentation nowadays
//! is generally not a method to manage memory, but an aspect in configuring CPU and its
//! surroundings.

pub mod idt;
pub mod ndt;

use paging::{LAddr, PAddr};

use core::alloc::Layout;
use core::mem::{size_of, transmute};
use core::ops::Range;
use core::ptr::null_mut;
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

/// Reload the Processor-Local Storage of the bootstrap CPU.
///
/// # Safety
///
/// The caller must ensure the value stored in [`archop::msr::FS_BASE`] is a
/// valid physical address.
pub unsafe fn reload_pls() -> LAddr {
      extern "C" {
            static TDATA_START: u8;
            static TBSS_START: u8;
      }
      use archop::msr;
      let pls_size = crate::KARGS.pls_layout.map_or(0, |layout| layout.size());

      let val = msr::read(msr::FS_BASE) as usize;
      if val != 0 {
            let ptr = PAddr::new(val).to_laddr(minfo::ID_OFFSET).cast::<usize>();
            let base = ptr.cast::<u8>().sub(pls_size);
            let size = (&TBSS_START as *const u8).offset_from(&TDATA_START) as usize;
            base.copy_from_nonoverlapping(&TDATA_START, size);
            base.add(size).write_bytes(0, pls_size - size);
            ptr.write(ptr as usize);

            msr::write(msr::FS_BASE, ptr as u64);
            LAddr::new(ptr.cast())
      } else {
            LAddr::new(null_mut())
      }
}

/// Allocate and initialize a new PLS for application CPU.
pub fn alloc_pls() -> *mut u8 {
      extern "C" {
            static TDATA_START: u8;
            static TBSS_START: u8;
      }

      let pls_layout = match crate::KARGS.pls_layout {
            Some(layout) => layout,
            None => return null_mut(),
      };

      unsafe {
            let base = alloc::alloc::alloc_zeroed(
                  pls_layout
                        .extend(Layout::new::<*mut u8>())
                        .expect("Failed to get the allocation layout")
                        .0,
            );

            if base.is_null() {
                  return null_mut();
            }

            let size = (&TBSS_START as *const u8).offset_from(&TDATA_START) as usize;
            base.copy_from_nonoverlapping(&TDATA_START, size);

            let self_ptr = base.add(pls_layout.size());
            self_ptr.cast::<*mut u8>().write(self_ptr);

            self_ptr
      }
}

/// Initialize segmentation structures
///
/// # Safety
///
/// The caller must ensure that this function is called only once from the bootstrap
/// CPU.
pub(super) unsafe fn init() -> (LAddr, LAddr) {
      let kernel_fs = unsafe { reload_pls() };
      let tss_rsp0 = ndt::init();
      idt::init();

      (tss_rsp0, kernel_fs)
}

pub(super) unsafe fn init_ap() -> (LAddr, LAddr) {
      let tss_rsp0 = ndt::init();
      idt::init();

      (
            tss_rsp0,
            LAddr::from(archop::msr::read(archop::msr::FS_BASE) as usize),
      )
}
