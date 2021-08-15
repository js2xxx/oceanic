use super::*;
use crate::mem::space::{krl, Flags, MemBlock};
use paging::LAddr;

use core::mem::size_of;
use core::pin::Pin;
use spin::Mutex;
use static_assertions::*;

pub const KRL_CODE_X64: SegSelector = SegSelector::from_const(0x08); // SegSelector::new().with_index(1)
pub const KRL_DATA_X64: SegSelector = SegSelector::from_const(0x10); // SegSelector::new().with_index(2)
pub const USR_CODE_X86: SegSelector = SegSelector::from_const(0x18); // SegSelector::new().with_index(3)
pub const USR_DATA_X64: SegSelector = SegSelector::from_const(0x20 + 3); // SegSelector::new().with_index(4).with_rpl(3)
pub const USR_CODE_X64: SegSelector = SegSelector::from_const(0x28 + 3); // SegSelector::new().with_index(5).with_rpl(3)

pub const INTR_CODE: SegSelector = SegSelector::from_const(0x08 + 4); // SegSelector::new().with_index(1).with_ti(true)
pub const INTR_DATA: SegSelector = SegSelector::from_const(0x10 + 4); // SegSelector::new().with_index(2).with_ti(true)

static mut GDT: Option<Mutex<DescTable<'static>>> = None;

#[thread_local]
static mut TSS: Option<Pin<&'static mut TssStruct>> = None;

/// Indicate a struct is a segment or gate descriptor.
pub trait Descriptor {}

/// The Task State Segment.
#[repr(C, packed)]
pub struct TssStruct {
      _rsvd1: u32,
      /// The legacy RSPs of different privilege levels.
      rsp: [u64; 3],
      _rsvd2: u64,
      /// The Interrupt Stack Tables.
      ist: [u64; 7],
      _rsvd3: u64,
      _rsvd4: u16,
      /// The IO base mappings.
      io_base: u16,
}

impl TssStruct {
      pub fn rsp0(&self) -> LAddr {
            LAddr::from(self.rsp[0] as usize)
      }

      pub fn io_base(&self) -> u16 {
            self.io_base
      }
}

/// A descriptor table.
pub struct DescTable<'a> {
      /// The base linear address of the table.
      base: LAddr,
      /// The end address of all the used entries of the table.
      end: LAddr,
      /// The number of how much entries the table can hold.
      capacity: usize,
      /// The memory block where the table is stored.
      _memory: Pin<&'a mut [MemBlock]>,
}

impl<'a> DescTable<'a> {
      /// Construct a new descriptor table.
      pub fn new(mut memory: Pin<&'a mut [MemBlock]>) -> DescTable {
            let base = LAddr::new(memory.as_mut_ptr().cast());
            let end_bound = LAddr::new(memory.as_mut_ptr_range().end.cast());
            let capacity = unsafe { end_bound.offset_from(*base) } as usize;

            DescTable {
                  base,
                  end: base,
                  capacity,
                  _memory: memory,
            }
      }

      /// Push back a descriptor with checks.
      ///
      /// # Errors
      ///
      /// If the table cannot hold one more requested descriptor, it'll return an error.
      pub fn push_checked<T: Descriptor>(&mut self, data: T) -> Result<u16, &'static str> {
            if self.end.val() + size_of::<T>() > self.base.val() + (self.capacity as usize) {
                  Err("Table full")
            } else {
                  let ptr = self.end.cast::<T>();
                  let ret = self.end.val() - self.base.val();

                  unsafe { ptr.write(data) };
                  self.end = LAddr::from(self.end.val() + size_of::<T>());
                  Ok(ret as u16)
            }
      }

      /// Push back a descriptor without check.
      ///
      /// NOTE: This function should only be called with constants.
      ///
      /// # Panics
      ///
      /// If the table cannot hold one more requested descriptor, it'll panic.
      #[inline]
      pub fn push<T: Descriptor>(&mut self, data: T) -> u16 {
            self.push_checked(data).unwrap()
      }

      /// Push back a [`Seg64`] without check.
      ///
      /// NOTE: This function should only be called with constants.
      ///
      /// # Panics
      ///
      /// If the table cannot hold one more requested descriptor, it'll panic.
      #[inline]
      pub fn push_s64(&mut self, base: u32, limit: u32, attr: u16, dpl: Option<u16>) -> u16 {
            self.push(Seg64::new(base, limit, attr, dpl))
      }

      /// Push back a [`Seg128`] without check.
      ///
      /// NOTE: This function should only be called with constants.
      ///
      /// # Panics
      ///
      /// If the table cannot hold one more requested descriptor, it'll panic.
      #[inline]
      pub fn push_s128(&mut self, base: LAddr, limit: u32, attr: u16, dpl: Option<u16>) -> u16 {
            self.push(Seg128::new(base, limit, attr, dpl))
      }

      // /// Return the iterator of the descriptor table.
      // #[inline]
      // pub fn iter(&self) -> DescEntryIter {
      //       DescEntryIter {
      //             distance: self.end - self.base,
      //             ptr: self.base as *mut u8,
      //       }
      // }

      /// Export the fat pointer of the descriptor table.
      ///
      /// # Safety
      ///
      /// The caller must ensure that the capacity of the descriptor table must be within the
      /// limit of `u16`.
      #[inline]
      pub unsafe fn export_fp(&self) -> FatPointer {
            FatPointer {
                  base: self.base,
                  limit: self.capacity as u16 - 1,
            }
      }
}

// /// An entry in a descriptor table.
// pub enum DescEntry<'a> {
//       S64(&'a mut Seg64),
//       S128(&'a mut Seg128),
// }

// /// The iterator of a descriptor table.
// pub struct DescEntryIter {
//       distance: usize,
//       ptr: *mut u8,
// }

// impl Iterator for DescEntryIter {
//       type Item = DescEntry;

//       fn next(&mut self) -> Option<Self::Item> {
//             let dtype = unsafe { get_type_attr(self.ptr) };
//             let size = match dtype {
//                   attrs::SEG_CODE | attrs::SEG_DATA => size_of::<u64>(),
//                   _ => size_of::<u128>(),
//             };
//             if size >= self.distance {
//                   None
//             } else {
//                   unsafe {
//                         self.distance -= size;
//                         self.ptr = self.ptr.add(size);
//                         let dtype = get_type_attr(self.ptr);
//                         match dtype {
//                               attrs::SEG_CODE | attrs::SEG_DATA => {
//                                     Some(DescEntry::S64(&mut *(self.ptr as *mut Seg64)))
//                               }
//                               attrs::SYS_LDT | attrs::SYS_TSS => {
//                                     Some(DescEntry::S128(&mut *(self.ptr as *mut Seg128)))
//                               }
//                               _ => None,
//                         }
//                   }
//             }
//       }
// }

/// All the segment descriptor that consumes a quadword.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seg64 {
      limit_low: u16,
      base_low: u16,
      base_mid: u8,
      attr_low: u8,
      attr_high_limit_high: u8,
      base_high: u8,
}
const_assert_eq!(size_of::<Seg64>(), size_of::<u64>());

impl Descriptor for Seg64 {}

impl Seg64 {
      /// Create a new segment descriptor with check.
      ///
      /// # Errors
      ///
      /// If the limit or the Descriptor Privilege Level is out of available range, it'll
      /// return an error.
      pub fn new_checked(
            base: u32,
            limit: u32,
            attr: u16,
            dpl: Option<u16>,
      ) -> Result<Seg64, SetError> {
            let dpl = dpl.unwrap_or(0);
            if !PL.contains(&dpl) || !LIMIT.contains(&limit) {
                  return Err(SetError::OutOfRange);
            }
            Ok(Seg64 {
                  limit_low: (limit & 0xFFFF) as _,
                  base_low: (base & 0xFFFF) as _,
                  base_mid: ((base >> 16) & 0xFF) as _,
                  attr_low: ((attr & 0xFF) | ((dpl & 3) << 5)) as _,
                  attr_high_limit_high: ((limit >> 16) & 0xF) as u8 | ((attr >> 8) & 0xF0) as u8,
                  base_high: ((base >> 24) & 0xFF) as _,
            })
      }

      /// Create a new segment descriptor without check.
      ///
      /// # Panics
      ///
      /// If the limit or the Descriptor Privilege Level is out of available range, it'll
      /// panic.
      pub fn new(base: u32, limit: u32, attr: u16, dpl: Option<u16>) -> Seg64 {
            Self::new_checked(base, limit, attr, dpl).unwrap()
      }
}

/// All the segment descriptor that consumes 2 quadwords.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seg128 {
      low: Seg64,
      base_higher: u32,
      _rsvd: u32,
}
const_assert_eq!(size_of::<Seg128>(), size_of::<u128>());

impl Descriptor for Seg128 {}

impl Seg128 {
      /// Create a new segment descriptor with check.
      ///
      /// # Errors
      ///
      /// If the limit or the Descriptor Privilege Level is out of available range, it'll
      /// return an error.
      pub fn new_checked(
            base: LAddr,
            limit: u32,
            attr: u16,
            dpl: Option<u16>,
      ) -> Result<Seg128, SetError> {
            let base = base.val();
            Ok(Seg128 {
                  low: Seg64::new_checked((base & 0xFFFFFFFF) as u32, limit, attr, dpl)?,
                  base_higher: (base >> 32) as u32,
                  _rsvd: 0,
            })
      }

      /// Create a new segment descriptor without check.
      ///
      /// # Panics
      ///
      /// If the limit or the Descriptor Privilege Level is out of available range, it'll
      /// panic.
      pub fn new(base: LAddr, limit: u32, attr: u16, dpl: Option<u16>) -> Seg128 {
            Self::new_checked(base, limit, attr, dpl).unwrap()
      }
}

/// Create a standard GDT for the kernel.
///
/// Construct a GDT object with the allocation provided by `space`. Return the GDT and its
/// kernel code & data selector.
///
/// NOTE: This function could only be called once from the BSP.
fn create_gdt() -> DescTable<'static> {
      let (layout, k) = paging::PAGE_LAYOUT
            .repeat(2)
            .expect("Failed to get the allocation size");
      assert!(k == paging::PAGE_SIZE);
      // SAFE: No physical address specified.
      let memory = unsafe {
            krl(|space| {
                  space.alloc_manual(
                        layout,
                        None,
                        Flags::READABLE | Flags::WRITABLE | Flags::ZEROED,
                  )
                  .expect("Failed to allocate memory for GDT")
            })
      }
      .expect("Kernel space uninitialized");

      let mut gdt = DescTable::new(memory);

      const LIM: u32 = 0xFFFFF;
      const ATTR: u16 = attrs::PRESENT | attrs::G4K;

      gdt.push_s64(0, 0, 0, None); // Null Desc
      let c64 = gdt.push_s64(0, LIM, attrs::SEG_CODE | attrs::X64 | ATTR, None);
      let d64 = gdt.push_s64(0, LIM, attrs::SEG_DATA | attrs::X64 | ATTR, None);
      let uc86 = gdt.push_s64(0, LIM, attrs::SEG_CODE | attrs::X86 | ATTR, None);
      let ud64 = gdt.push_s64(0, LIM, attrs::SEG_DATA | attrs::X64 | ATTR, Some(3));
      let uc64 = gdt.push_s64(0, LIM, attrs::SEG_CODE | attrs::X64 | ATTR, Some(3));

      assert!(c64 >> 3 == KRL_CODE_X64.index());
      assert!(d64 >> 3 == KRL_DATA_X64.index());
      assert!(uc86 >> 3 == USR_CODE_X86.index());
      assert!(ud64 >> 3 == USR_DATA_X64.index());
      assert!(uc64 >> 3 == USR_CODE_X64.index());

      gdt
}

/// Load a GDT into x86 architecture's `gdtr` and reset all the segment registers according
/// to it.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
///
/// The caller must ensure that `gdt` is a valid GDT object and `krl_sel` consists of the
/// kernel's code & data selector in `gdt`.
unsafe fn load_gdt(gdt: &DescTable) {
      extern "C" {
            fn reset_seg(code: SegSelector, data: SegSelector);
      }

      let gdtr = gdt.export_fp();
      asm!("lgdt [{}]", in(reg) &gdtr);

      reset_seg(KRL_CODE_X64, KRL_DATA_X64);
}

/// Create a standard LDT.
///
/// Construct an LDT object with the allocation provided by `space`.
///
/// The Local Descriptor Table is used to indicate whether the code is inside a interrupt
/// routine. In the assembly code, we can check the `TI` bit in `cs`.
///
/// This function returns the LDT, its address & size, and its code selector.
///
/// NOTE: This function should only be called once from the BSP.
fn create_ldt() -> (DescTable<'static>, (LAddr, usize)) {
      // SAFE: No physical address specified.
      let mut memory = unsafe {
            krl(|space| {
                  space.alloc_manual(
                        paging::PAGE_LAYOUT,
                        None,
                        Flags::READABLE | Flags::WRITABLE | Flags::ZEROED,
                  )
                  .expect("Failed to allocate memory for LDT")
            })
      }
      .expect("Kernel space uninitialized");

      let ldt_ptr = (
            LAddr::new(memory.as_mut_ptr().cast()),
            size_of::<Seg64>() * 3,
      );

      let mut ldt = DescTable::new(memory);

      const LIM: u32 = 0xFFFFF;
      const ATTR: u16 = attrs::PRESENT | attrs::G4K;

      ldt.push_s64(0, 0, 0, None); // Null Desc
      let code = ldt.push_s64(0, LIM, attrs::SEG_CODE | attrs::X64 | ATTR, None);
      let data = ldt.push_s64(0, LIM, attrs::SEG_DATA | attrs::X64 | ATTR, None);
      assert!(code >> 3 == INTR_CODE.index());
      assert!(data >> 3 == INTR_DATA.index());

      // In LDT: bitfield ti = 1
      (ldt, ldt_ptr)
}

/// Push an LDT into a GDT.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
///
/// The caller must ensure that `gdt` is a valid GDT, `ldt_ptr` consists of valid
/// LDT's address & size, and the GDT does not contain the specified LDT.
unsafe fn push_ldt(gdt: &mut DescTable, ldt_ptr: (LAddr, usize)) -> SegSelector {
      let (base, size) = ldt_ptr;

      let ldtr = gdt.push_s128(
            base,
            (size - 1) as u32,
            attrs::SYS_LDT | attrs::PRESENT,
            None,
      );

      SegSelector::from(ldtr)
}

/// Load an LDT into x86 architecture's `ldtr`.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
///
/// The caller must ensure that `ldtr` points to a valid LDT and its GDT is loaded.
unsafe fn load_ldt(ldtr: SegSelector) {
      asm!("lldt [{}]", in(reg) &ldtr);
}

/// Create a new TSS structure.
///
/// This function returns the new structure and its base address.
fn create_tss() -> (Pin<&'static mut TssStruct>, LAddr) {
      // SAFE: No physical address specified.
      let alloc_stack = || unsafe {
            let (layout, k) = paging::PAGE_LAYOUT
                  .repeat(4)
                  .expect("Failed to calculate the layout");
            assert!(k == paging::PAGE_SIZE);
            let memory = krl(|space| {
                  space.alloc_manual(
                        layout,
                        None,
                        Flags::READABLE | Flags::WRITABLE | Flags::ZEROED,
                  )
                  .expect("Failed to allocate stack")
            })
            .expect("Kernel space uninitialized");

            memory.as_ptr().cast::<u8>().add(layout.size())
      };

      let rsp0 = alloc_stack();
      let ist1 = alloc_stack();

      // SAFE: No physical address specified.
      let mut memory = unsafe {
            krl(|space| {
                  space.alloc_typed::<TssStruct>(
                        None,
                        Flags::READABLE | Flags::WRITABLE | Flags::ZEROED,
                  )
                  .expect("Failed to allocate TSS")
            })
      }
      .expect("Kernel space uninitialized");

      let base = memory.as_mut_ptr();

      // SAFE: `base` points to a valid address.
      unsafe {
            base.write(TssStruct {
                  _rsvd1: 0,
                  // The legacy RSPs of different privilege levels.
                  rsp: [rsp0 as u64, 0, 0],
                  _rsvd2: 0,
                  // The Interrupt Stack Tables.
                  ist: [ist1 as u64, 0, 0, 0, 0, 0, 0],
                  _rsvd3: 0,
                  _rsvd4: 0,
                  // The IO base mappings.
                  io_base: 0,
            });
      }

      // SAFE: A valid TSS structure is constructed in the memory block.
      let tss = unsafe { memory.map_unchecked_mut(|u| u.assume_init_mut()) };

      (tss, LAddr::new(base.cast()))
}

/// Push a TSS structure to the GDT.
///
/// This function returns the selector to the TSS structure.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
///
/// The caller must ensure that `gdt` is a valid GDT and `tss_base` points to a valid
/// TSS structure.
unsafe fn push_tss(gdt: &mut DescTable, tss_base: LAddr) -> SegSelector {
      let tr = gdt.push_s128(
            LAddr::new(tss_base.cast()),
            (size_of::<TssStruct>() - 1) as u32,
            attrs::SYS_TSS | attrs::PRESENT,
            Some(3),
      );

      SegSelector::from(tr)
}

/// Load an TSS into x86 architecture's `tr`.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
///
/// The caller must ensure that `tr` points to a valid TSS and its GDT is loaded.
unsafe fn load_tss(tr: SegSelector) {
      unsafe { asm!("ltr [{}]", in(reg) &tr) };
}

/// Initialize NDT (GDT & LDT & TSS) in x86 architecture by the bootstrap CPU.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
///
/// The caller must ensure that this function is called only once from the bootstrap CPU.
pub unsafe fn init() -> (LAddr, LAddr) {
      let mut gdt = ndt::create_gdt();
      unsafe { ndt::load_gdt(&gdt) };
      let kernel_fs = unsafe { reload_pls() };

      let (ldt, ldt_ptr) = ndt::create_ldt();
      let ldtr = unsafe { ndt::push_ldt(&mut gdt, ldt_ptr) };
      unsafe { ndt::load_ldt(ldtr) };

      let (tss, tss_base) = ndt::create_tss();
      let tr = unsafe { ndt::push_tss(&mut gdt, tss_base) };
      unsafe { ndt::load_tss(tr) };

      GDT = Some(Mutex::new(gdt));
      let tss_rsp0 = tss.rsp0();
      TSS = Some(tss);

      // Manually drop the reference to LDT without dropping the data because those structures are
      // no longer needed to be referenced by the code.
      let _ = ManuallyDrop::new(ldt);

      (tss_rsp0, kernel_fs)
}
