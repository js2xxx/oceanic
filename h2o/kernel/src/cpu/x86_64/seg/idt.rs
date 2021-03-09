use super::*;

use paging::LAddr;

use core::mem::{size_of, transmute};
use core::ops::{Index, IndexMut, Range};
use core::slice::{Iter, IterMut};
// use spin::Mutex;
use static_assertions::*;

/// The count of all the interrupts in one CPU.
///
/// This is limited by `int /imm8` assembly instruction.
const NR_INTRS: usize = 256;

/// The range of all the allocable (usable for custom) interrupts in one CPU.
///
/// NOTE: `0..32` is reserved for exceptions.
const ALLOCABLE_INTRS: Range<usize> = 32..NR_INTRS;

/// The gate descriptor.
///
/// There's no gate descriptor that consumes only one quadword because Task Gates are invalid
/// in long (x86_64) mode.
#[repr(C, packed)]
#[derive(Builder, Clone, Copy, Debug, PartialEq, Eq)]
#[builder(no_std, build_fn(validate = "Self::validate"))]
pub struct Gate {
      #[builder(private, default)]
      offset_low: u16,
      #[builder(default)]
      selector: SegSelector,
      #[builder(default)]
      ist: u8,
      #[builder(private, default)]
      attr: u8,
      #[builder(private, default)]
      offset_mid: u16,
      #[builder(private, default)]
      offset_high: u32,
      #[builder(setter(skip), default)]
      _rsvd: u32,
}
const_assert_eq!(size_of::<Gate>(), size_of::<u128>());

/// The IDT structure.
#[repr(align(0x10))]
pub struct IntDescTable([Gate; NR_INTRS]);

impl GateBuilder {
      /// Set up the offset of a gate descriptor.
      pub fn offset(&mut self, offset: LAddr) -> &mut Self {
            let offset = offset.val();
            self.offset_low((offset & 0xFFFF) as _)
                  .offset_mid(((offset >> 16) & 0xFFFF) as _)
                  .offset_high((offset >> 32) as _)
      }

      /// Set up the attributes - type and DPL of a gate descriptor.
      pub fn attribute(&mut self, attr: u16, dpl: u16) -> &mut Self {
            self.attr((attr & 0xFF) as u8 | ((dpl & 3) << 5) as u8)
      }

      /// Check if the init data is valid.
      fn validate(&self) -> Result<(), &'static str> {
            if let Some(ist) = self.ist {
                  if !IST.contains(&ist) {
                        return Err("Invalid IST");
                  }
            }

            Ok(())
      }
}

impl Gate {
      /// Construct a zeroed gate descriptor.
      #[allow(clippy::transmuting_null)]
      pub const fn zeroed() -> Gate {
            Gate {
                  offset_low: 0,
                  selector: unsafe { transmute(0_u16) },
                  ist: 0,
                  attr: 0,
                  offset_mid: 0,
                  offset_high: 0,
                  _rsvd: 0,
            }
      }

      /// Check if the descriptor is a interrupt gate.
      pub fn is_int(&self) -> bool {
            self.attr as u16 == attrs::PRESENT | attrs::INT_GATE
      }

      /// Check if the descriptor is a trap gate.
      pub fn is_trap(&self) -> bool {
            self.attr as u16 == attrs::PRESENT | attrs::TRAP_GATE
      }

      /// Check if the descriptor is valid.
      pub fn is_valid(&self) -> bool {
            self.is_int() || self.is_trap()
      }

      /// Get the offset of the descriptor.
      pub fn get_offset(&self) -> LAddr {
            LAddr::from(
                  (self.offset_low as usize)
                        | ((self.offset_mid as usize) << 16)
                        | ((self.offset_high as usize) << 32),
            )
      }
}

impl Index<usize> for IntDescTable {
      type Output = Gate;
      fn index(&self, index: usize) -> &Self::Output {
            &self.0[index]
      }
}

impl IndexMut<usize> for IntDescTable {
      fn index_mut(&mut self, index: usize) -> &mut Self::Output {
            &mut self.0[index]
      }
}

impl IntDescTable {
      /// Construct a new (zeroed) IDT.
      pub fn new() -> IntDescTable {
            IntDescTable(unsafe { core::mem::zeroed() })
      }

      /// Export the fat pointer of the IDT.
      pub fn export_fp(&self) -> FatPointer {
            let base = LAddr::new(self.0.as_ptr().cast::<u8>() as *mut _);
            let size = self.0.len() * size_of::<Gate>();
            FatPointer {
                  base,
                  limit: (size - 1) as u16,
            }
      }

      /// Return the iterator of the IDT.
      pub fn iter(&self) -> Iter<Gate> {
            self.0.iter()
      }

      /// Return the mutable iterator of the IDT.
      pub fn iter_mut(&mut self) -> IterMut<Gate> {
            self.0.iter_mut()
      }

      /// Allocate a free slot (position of gate descriptor) in the IDT.
      pub fn alloc(&self) -> Option<usize> {
            self.iter()
                  .enumerate()
                  .find(|x| !x.1.is_valid() && ALLOCABLE_INTRS.contains(&x.0))
                  .map(|x| x.0)
      }

      /// Deallocate (destroy) a gate descriptor in the IDT.
      pub fn dealloc(&mut self, idx: usize) -> Result<(), &'static str> {
            if !(0..NR_INTRS).contains(&idx) {
                  return Err("Index out of range");
            }
            self[idx] = Gate::zeroed();
            Ok(())
      }
}

// /// The per-cpu IDT.
// #[thread_local]
// pub static IDT: Mutex<IntDescTable> = Mutex::new(IntDescTable::new());

// /// Initialize the per-cpu IDT.
// pub fn init_idt() {
//       let idt = IDT.lock();

//       unsafe {
//             let ptr = idt.export_fp();
//             asm!("cli; lidt [{}]", in(reg) &ptr);
//       }
// }
