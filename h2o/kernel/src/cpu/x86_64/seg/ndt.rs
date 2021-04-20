use super::*;
use crate::mem::space::{Flags, MemBlock, Space};
use paging::LAddr;

use alloc::sync::Arc;
use core::mem::size_of;
use core::pin::Pin;
use spin::Mutex;
use static_assertions::*;

/// Indicate a struct is a segment or gate descriptor.
pub trait Descriptor {}

/// The Task State Segment
#[repr(C, packed)]
pub struct TssStruct {
      _rsvd1: u32,
      /// The legacy RSPs of different privilege levels
      rsp: [u64; 3],
      _rsvd2: u64,
      /// The Interrupt Stack Tables
      ist: [u64; 7],
      _rsvd3: u64,
      _rsvd4: u16,
      /// The IO base mappings
      iobase: u16,
}

/// A descriptor table.
pub struct DescTable<'a> {
      /// The base linear address of the table.
      base: LAddr,
      /// The end address of all the used entries of the table.
      end: LAddr,
      /// The number of how much entries the table can hold.
      capacity: usize,

      memory: Pin<&'a mut [MemBlock]>,
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
                  memory,
            }
      }

      /// Push back a descriptor with checks.
      ///
      /// # Errors
      ///
      /// If the table cannot hold one more requested descriptor, it'll return an error.
      pub fn push_back_checked<T: Descriptor>(&mut self, data: T) -> Result<u16, &'static str> {
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
      pub fn push_back<T: Descriptor>(&mut self, data: T) -> u16 {
            self.push_back_checked(data).unwrap()
      }

      /// Push back a [`Seg64`] without check.
      ///
      /// NOTE: This function should only be called with constants.
      ///
      /// # Panics
      ///
      /// If the table cannot hold one more requested descriptor, it'll panic.
      #[inline]
      pub fn push_back_s64(&mut self, base: u32, limit: u32, attr: u16, dpl: Option<u16>) -> u16 {
            self.push_back(Seg64::new(base, limit, attr, dpl))
      }

      /// Push back a [`Seg128`] without check.
      ///
      /// NOTE: This function should only be called with constants.
      ///
      /// # Panics
      ///
      /// If the table cannot hold one more requested descriptor, it'll panic.
      #[inline]
      pub fn push_back_s128(
            &mut self,
            base: LAddr,
            limit: u32,
            attr: u16,
            dpl: Option<u16>,
      ) -> u16 {
            self.push_back(Seg128::new(base, limit, attr, dpl))
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

/// Initialize the GDT.
///
/// NOTE: This function sould only be called once from the BSP.
pub fn init_gdt(space: &Arc<Space>) -> Mutex<DescTable<'_>> {
      extern "C" {
            fn reset_seg(code: SegSelector, data: SegSelector);
      }

      let (layout, k) = paging::PAGE_LAYOUT
            .repeat(2)
            .expect("Failed to get the allocation size");
      assert!(k == paging::PAGE_SIZE);
      let memory =
            unsafe { space.alloc_manual(layout, None, true, Flags::READABLE | Flags::WRITABLE) }
                  .expect("Failed to allocate memory for GDT");

      let gdt = Mutex::new(DescTable::new(memory));
      let mut gdt_data = gdt.lock();

      const LIM: u32 = 0xFFFFF;
      const ATTR: u16 = attrs::PRESENT | attrs::G4K;

      gdt_data.push_back_s64(0, 0, 0, None); // Null Desc
      let code = gdt_data.push_back_s64(0, LIM, attrs::SEG_CODE | attrs::X64 | ATTR, None);
      let data = gdt_data.push_back_s64(0, LIM, attrs::SEG_DATA | attrs::X64 | ATTR, None);
      gdt_data.push_back_s64(0, LIM, attrs::SEG_CODE | attrs::X86 | ATTR, None);
      gdt_data.push_back_s64(0, LIM, attrs::SEG_DATA | attrs::X86 | ATTR, None);
      gdt_data.push_back_s64(0, LIM, attrs::SEG_CODE | attrs::X64 | ATTR, Some(3));
      gdt_data.push_back_s64(0, LIM, attrs::SEG_DATA | attrs::X64 | ATTR, Some(3));
      gdt_data.push_back_s64(0, LIM, attrs::SEG_CODE | attrs::X86 | ATTR, Some(3));
      gdt_data.push_back_s64(0, LIM, attrs::SEG_DATA | attrs::X86 | ATTR, Some(3));

      unsafe {
            let gdtr = gdt_data.export_fp();
            asm!("lgdt [{}]", in(reg) &gdtr);
      }

      unsafe {
            let (code, data) = (SegSelector::from(code), SegSelector::from(data));
            reset_seg(code, data);
      }

      drop(gdt_data);
      gdt
}

// /// Initialize the GDT for APs.
// pub fn init_gdt_ap() {
//       let gdt = GDT.lock();
//       let gdtr = gdt.export_fp();
//       unsafe { asm!("lgdt [{}]", in(reg) &gdtr) };
// }

// /// Initialize the LDT.
// ///
// /// NOTE: This function should only be called once from the BSP.
// pub fn init_ldt() {
//       let (base, _, capacity) = Vec::<Seg64>::with_capacity(3).into_raw_parts();
//       let (base, capacity) = (base as LAddr, (capacity * size_of::<Seg64>()) as u16);

//       let mut ldt = LDT.lock();
//       ldt.0 = DescTable::new(base, capacity);

//       const LIM: u32 = 0xFFFFF;
//       const ATTR: u16 = attrs::PRESENT | attrs::G4K;

//       ldt.0.push_back_s64(0, 0, 0, None); // Null Desc
//       ldt.0.push_back_s64(0, LIM, attrs::SEG_CODE | attrs::X64 | ATTR, None);
//       ldt.0.push_back_s64(0, LIM, attrs::SEG_DATA | attrs::X64 | ATTR, None);

//       unsafe {
//             ldt.1 = {
//                   let mut gdt = GDT.lock();
//                   gdt.push_back_s128(
//                         base,
//                         (capacity - 1) as u32,
//                         attrs::SYS_LDT | attrs::PRESENT,
//                         None,
//                   )
//             };
//             asm!("lldt [{}]", in(reg) &ldt.1);
//       }
// }

// /// Initialize the LDT for APs.
// pub fn init_ldt_ap() {
//       let ldt = LDT.lock();
//       let ldtr = ldt.1;
//       unsafe { asm!("lldt [{}]", in(reg) &ldtr) };
// }

// /// Initialize the TSS of the current CPU.
// pub fn init_tss() {
//       let (rsp0, _, _) = Vec::<u8>::with_capacity(PAGE_SIZE * 4).into_raw_parts();
//       let (ist1, _, _) = Vec::<u8>::with_capacity(PAGE_SIZE * 4).into_raw_parts();

//       unsafe {
//             TSS.rsp[0] = rsp0 as u64;
//             TSS.ist[0] = ist1 as u64;

//             let base = &TSS as *const TssStruct as LAddr;
//             let tr = {
//                   let mut gdt = GDT.lock();
//                   gdt.push_back_s128(
//                         base,
//                         (size_of_val(&TSS) - 1) as u32,
//                         attrs::SYS_TSS | attrs::PRESENT,
//                         Some(3),
//                   )
//             };

//             asm!("ltr [{}]", in(reg) &tr);
//       }
// }

// pub fn get_tss_rsp0() -> u64 {
//       unsafe { TSS.rsp[0] }
// }

// pub fn get_tss_iobase() -> u16 {
//       unsafe { TSS.iobase }
// }
