use super::{DelivMode, TriggerMode};
use crate::cpu::arch::apic::{ipi, lapic};
use crate::cpu::arch::seg::ndt::Seg64;
use crate::cpu::arch::tsc::{delay, ns_clock};
use crate::mem::space::{krl, Flags};
use acpi::table::madt::LapicNode;
use paging::PAddr;

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use modular_bitfield::prelude::*;

// pub fn ipi_handler() {}

#[derive(Debug, Clone, Copy, BitfieldSpecifier)]
#[repr(u64)]
pub enum Shorthand {
      None = 0,
      Me = 1,
      All = 2,
      Others = 3,
}

#[derive(Clone, Copy)]
#[bitfield]
pub struct IcrEntry {
      pub(super) vec: u8,
      #[bits = 3]
      pub(super) deliv_mode: DelivMode,
      pub(super) dest_logical: bool,
      pub(super) pending: bool,
      #[skip]
      __: B1,
      pub(super) level_assert: bool,
      #[bits = 1]
      pub(super) trigger_mode: TriggerMode,
      #[skip]
      __: B2,
      pub(super) shorthand: Shorthand,
      #[skip]
      __: B12,
}

impl From<u32> for IcrEntry {
      fn from(x: u32) -> Self {
            Self::from_bytes(x.to_ne_bytes())
      }
}

impl From<IcrEntry> for u32 {
      fn from(x: IcrEntry) -> Self {
            Self::from_ne_bytes(x.into_bytes())
      }
}

#[repr(C)]
pub struct TramSubheader {
      stack: u64,
      pgc: u64,
      tls: u64,
}

#[repr(C)]
pub struct TramHeader {
      booted: AtomicBool,
      subheader: UnsafeCell<TramSubheader>,
      kmain: *mut u8,
      init_efer: u64,
      init_cr4: u64,
      init_cr0: u64,
      gdt: [Seg64; 3],
}

impl TramHeader {
      pub unsafe fn new() -> TramHeader {
            use archop::{msr, reg};

            TramHeader {
                  booted: AtomicBool::new(true),
                  subheader: UnsafeCell::new(core::mem::zeroed()),
                  kmain: crate::kmain_ap as *mut _,
                  init_efer: msr::read(msr::EFER),
                  init_cr4: reg::cr4::read(),
                  init_cr0: reg::cr0::read(),
                  gdt: {
                        use crate::cpu::arch::seg::attrs;
                        const LIM: u32 = 0xFFFFF;
                        const ATTR: u16 = attrs::PRESENT | attrs::G4K;

                        [
                              Seg64::new(0, 0, 0, None),
                              Seg64::new(0, LIM, attrs::SEG_CODE | attrs::X64 | ATTR, None),
                              Seg64::new(0, LIM, attrs::SEG_DATA | attrs::X64 | ATTR, None),
                        ]
                  },
            }
      }

      pub unsafe fn reset_subheader(&self, start: u64) {
            while !self.booted.swap(false, Ordering::SeqCst) && ns_clock() - start < 1000000 {
                  archop::pause();
            }

            let stack = unsafe {
                  let (layout, _) = paging::PAGE_LAYOUT
                        .repeat(16)
                        .expect("Failed to create a layout");
                  let mut memory = krl(|space| {
                        space.alloc_manual(layout, None, Flags::READABLE | Flags::WRITABLE)
                              .expect("Failed to allocate stack for AP")
                  })
                  .expect("Kernel space uninitialized");
                  memory.as_mut_ptr_range().end
            } as u64;

            let ptr = self.subheader.get();
            ptr.write(TramSubheader {
                  stack,
                  pgc: 0,
                  tls: 0,
            });
      }
}

/// # Safety
///
/// This function must be called after Local APIC initialization.
pub unsafe fn start_cpu(lapics: Vec<LapicNode>) {
      static TRAM_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tram"));

      let base_phys = PAddr::new(minfo::TRAMPOLINE_RANGE.start);
      let base = base_phys.to_laddr(minfo::ID_OFFSET);

      let ptr = *base;

      unsafe {
            let slice = core::slice::from_raw_parts_mut(ptr, TRAM_DATA.len());
            slice.copy_from_slice(TRAM_DATA);
      }

      let header = {
            let header = ptr.add(16).cast::<TramHeader>();
            unsafe { header.write(TramHeader::new()) };
            &*header
      };

      let mut start = 0;
      for LapicNode { id, .. } in lapics.iter() {
            header.reset_subheader(start);

            lapic(|lapic| {
                  lapic.send_ipi(0, DelivMode::Init, ipi::Shorthand::None, *id);
                  delay(Duration::from_millis(50));

                  lapic.send_ipi(
                        (*base_phys >> 3) as u8,
                        DelivMode::StartUp,
                        ipi::Shorthand::None,
                        *id,
                  );
            });

            start = ns_clock();
      }
}
