use super::ndt::INTR_CODE;
use super::*;
use crate::cpu::arch::intr::def::{IdtEntry, IdtInit, IDT_INIT};
use paging::LAddr;

use core::mem::size_of;
use core::ops::{Index, IndexMut, Range};
// use core::slice::{Iter, IterMut};
use spin::Lazy;
use static_assertions::*;

/// The count of all the interrupts in one CPU.
///
/// This is limited by `int /imm8` assembly instruction.
const NR_INTRS: usize = 256;

/// The range of all the allocable (usable for custom) interrupts in one CPU.
///
/// NOTE: `0..32` is reserved for exceptions.
const ALLOCABLE_INTRS: Range<usize> = 32..NR_INTRS;

#[thread_local]
static IDT: Lazy<IntDescTable> = Lazy::new(|| {
      let mut array = [Gate::zeroed(); NR_INTRS];

      let mut set_ent = |entry: &IdtEntry| {
            let desc = GateBuilder::new()
                  .offset(LAddr::new(entry.entry as *mut u8))
                  .selector(INTR_CODE)
                  .attribute(attrs::INT_GATE | attrs::PRESENT, entry.dpl)
                  .ist(entry.ist)
                  .build()
                  .expect("Failed to build a gate descriptor");

            array[entry.vec as u16 as usize] = desc;
      };

      for init in IDT_INIT {
            match init {
                  IdtInit::Single(ent) => set_ent(ent),
                  IdtInit::Multiple(entries) => {
                        for ent in entries.iter() {
                              set_ent(ent);
                        }
                  }
            }
      }

      IntDescTable(array)
});

/// The gate descriptor.
///
/// There's no gate descriptor that consumes only one quadword because Task Gates are invalid
/// in long (x86_64) mode.
///
/// ## Actual Fields of structure
///
/// Because a packed & aligned structure cannot be built in Rust, so we hide the actual fields
/// in 2 quadwords.
///
///     size: |<-------u16------>|<-------u16------>|<--u8-->|<---u8-->|<-------u16------>|
///     `q0`: |   offset_low     |     selector     |   IST  |  attr   |   offset_mid     |
///     `q1`: |             offset_high             |             (reserved)              |
#[repr(C, align(0x10))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gate {
      q0: u64,
      q1: u64,
}
const_assert_eq!(size_of::<Gate>(), size_of::<u128>());

pub struct GateBuilder {
      offset_low: u16,
      selector: SegSelector,
      ist: u8,
      attr: u8,
      offset_mid: u16,
      offset_high: u32,
}

impl GateBuilder {
      pub fn new() -> Self {
            unsafe { core::mem::zeroed() }
      }
      /// Set up the offset of the gate descriptor.
      pub fn offset(&mut self, offset: LAddr) -> &mut Self {
            let offset = offset.val();
            self.offset_low = (offset & 0xFFFF) as _;
            self.offset_mid = ((offset >> 16) & 0xFFFF) as _;
            self.offset_high = (offset >> 32) as _;
            self
      }

      /// Set up the attributes - type and DPL of the gate descriptor.
      pub fn attribute(&mut self, attr: u16, dpl: u16) -> &mut Self {
            self.attr = (attr & 0xFF) as u8 | ((dpl & 3) << 5) as u8;
            self
      }

      /// Set up the selector of the gate descriptor.
      pub fn selector(&mut self, selector: SegSelector) -> &mut Self {
            self.selector = selector;
            self
      }

      /// Set up the IST index of the gate descriptor.
      pub fn ist(&mut self, ist: u8) -> &mut Self {
            self.ist = ist;
            self
      }

      /// Check if the init data is valid.
      fn validate(&self) -> Result<(), &'static str> {
            if self.ist != 0 && !IST.contains(&self.ist) {
                  return Err("Invalid IST");
            }

            Ok(())
      }

      /// Build the descriptor.
      pub fn build(&mut self) -> Result<Gate, &'static str> {
            self.validate()?;
            Ok(Gate {
                  q0: (self.offset_low as u64)
                        | ((self.selector.into_val() as u64) << 16)
                        | ((self.ist as u64) << 32)
                        | ((self.attr as u64) << 40)
                        | ((self.offset_mid as u64) << 48),
                  q1: self.offset_high as u64,
            })
      }
}

impl Gate {
      /// Construct a zeroed gate descriptor.
      pub const fn zeroed() -> Gate {
            Gate { q0: 0, q1: 0 }
      }

      #[inline]
      fn attr(&self) -> u16 {
            ((self.q0 >> 40) & 0xFF) as u16
      }

      /// Check if the descriptor is a interrupt gate.
      pub fn is_int(&self) -> bool {
            self.attr() == attrs::PRESENT | attrs::INT_GATE
      }

      /// Check if the descriptor is a trap gate.
      pub fn is_trap(&self) -> bool {
            self.attr() == attrs::PRESENT | attrs::TRAP_GATE
      }

      /// Check if the descriptor is valid.
      pub fn is_valid(&self) -> bool {
            self.is_int() || self.is_trap()
      }

      /// Get the offset of the descriptor.
      pub fn get_offset(&self) -> LAddr {
            LAddr::from(
                  ((self.q0 & 0xFFFF) as usize)
                        | (((self.q0 >> 32) & 0xFFFF0000) as usize)
                        | ((self.q1 as usize) << 32),
            )
      }
}

pub type IdtArray = [Gate; NR_INTRS];

/// The IDT structure.
#[repr(align(4096))]
pub struct IntDescTable(IdtArray);

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
      pub fn new(array: IdtArray) -> Self {
            IntDescTable(array)
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

      // /// Return the iterator of the IDT.
      // pub fn iter(&self) -> Iter<Gate> {
      //       self.0.iter()
      // }

      // /// Return the mutable iterator of the IDT.
      // pub fn iter_mut(&mut self) -> IterMut<Gate> {
      //       self.0.iter_mut()
      // }

      // /// Allocate a free slot (position of gate descriptor) in the IDT.
      // pub fn alloc(&self) -> Option<usize> {
      //       self.iter()
      //             .enumerate()
      //             .find(|x| !x.1.is_valid() && ALLOCABLE_INTRS.contains(&x.0))
      //             .map(|x| x.0)
      // }

      // /// Deallocate (destroy) a gate descriptor in the IDT.
      // pub fn dealloc(&mut self, idx: usize) -> Result<(), &'static str> {
      //       if !(0..NR_INTRS).contains(&idx) {
      //             return Err("Index out of range");
      //       }
      //       self[idx] = Gate::zeroed();
      //       Ok(())
      // }
}

/// Load an IDT into x86 architecture's `idtr`.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
///
/// The caller must ensure that `idt` is a valid LDT.
unsafe fn load_idt(idt: &IntDescTable) {
      let idtr = idt.export_fp();
      asm!("cli; lidt [{}]", in(reg) &idtr);
}

/// Initialize IDT in x86 architecture by the bootstrap CPU.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure to make
/// preparations.
pub unsafe fn init() {
      unsafe { idt::load_idt(&IDT) };
}
