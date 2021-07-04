//! TODO: Define Local APIC's interrupt vectors.

use super::ctx::Frame;

use core::ops::Range;

pub const NR_VECTORS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ExVec {
      DivideBy0 = 0,
      Debug = 1,
      Nmi = 2,
      Breakpoint = 3,
      Overflow = 4,
      Bound = 5,
      InvalidOp = 6,
      DeviceNa = 7,
      DoubleFault = 8,
      CoprocOverrun = 9,
      InvalidTss = 0xA,
      SegmentNa = 0xB,
      StackFault = 0xC,
      GeneralProt = 0xD,
      PageFault = 0xE,
      FloatPoint = 0x10,
      Alignment = 0x11,
      MachineCheck = 0x12,
      SimdExcep = 0x13,
      // Virtual = 0x14,
      // ControlProt = 0x15,
      // VmmComm = 0x1D,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ApicVec {
      Timer = 0x20,
      Ipi = 0x21,
      Error = 0x22,
      Spurious = 0xFF,
}

pub const ALLOC_VEC: Range<u16> = 0x40..0xFF;

type IdtRoute = unsafe extern "C" fn();

macro_rules! single_ent {
      ($vec:expr, $name:ident, $ist:expr, $dpl:expr) => {
            Single(IdtEntry::new(
                  $vec as u16,
                  paste::paste! {[<rout_ $name>]},
                  $ist,
                  $dpl,
            ))
      };
}

#[macro_export]
macro_rules! hdl {
      ($name:ident, |$frame_arg:ident| $body:block) => {
            paste::paste! {
                  extern "C" { pub fn [<rout_ $name>](); }
                  #[no_mangle]
                  unsafe extern "C" fn [<hdl_ $name>]($frame_arg: *mut Frame) $body
            }
      };
}

pub enum IdtInit {
      Single(IdtEntry),
      Multiple(&'static [IdtEntry]),
}
use IdtInit::*;

pub struct IdtEntry {
      pub vec: u16,
      pub entry: IdtRoute,
      pub ist: u8,
      pub dpl: u16,
}

impl IdtEntry {
      pub const fn new(vec: u16, entry: IdtRoute, ist: u8, dpl: u16) -> Self {
            IdtEntry {
                  vec,
                  entry,
                  ist,
                  dpl,
            }
      }
}

pub static IDT_INIT: &[IdtInit] = &[
      // x86 exceptions
      single_ent!(ExVec::DivideBy0, div_0, 0, 0),
      single_ent!(ExVec::Debug as u16, debug, 0, 0),
      single_ent!(ExVec::Nmi as u16, nmi, 0, 0),
      single_ent!(ExVec::Breakpoint as u16, breakpoint, 0, 0),
      single_ent!(ExVec::Overflow as u16, overflow, 0, 3),
      single_ent!(ExVec::Bound as u16, bound, 0, 0),
      single_ent!(ExVec::InvalidOp as u16, invalid_op, 0, 0),
      single_ent!(ExVec::DeviceNa as u16, device_na, 0, 0),
      single_ent!(ExVec::DoubleFault as u16, double_fault, 0, 0),
      single_ent!(ExVec::CoprocOverrun as u16, coproc_overrun, 0, 0),
      single_ent!(ExVec::InvalidTss as u16, invalid_tss, 0, 0),
      single_ent!(ExVec::SegmentNa as u16, segment_na, 0, 0),
      single_ent!(ExVec::StackFault as u16, stack_fault, 0, 0),
      single_ent!(ExVec::GeneralProt as u16, general_prot, 0, 0),
      single_ent!(ExVec::PageFault as u16, page_fault, 0, 0),
      single_ent!(ExVec::FloatPoint as u16, fp_excep, 0, 0),
      single_ent!(ExVec::Alignment as u16, alignment, 0, 0),
      // single_ent!(ExVec::MachineCheck as u16, mach_check, 0, 0),
      single_ent!(ExVec::SimdExcep as u16, simd, 0, 0),
      // Local APIC interrupts
      single_ent!(ApicVec::Timer as u16, lapic_timer, 0, 0),
      single_ent!(ApicVec::Spurious as u16, lapic_spurious, 0, 0),
      single_ent!(ApicVec::Error as u16, lapic_error, 0, 0),
      // All other allocable interrupts
      Multiple(repeat::repeat! {"&[" for i in 0x40..0xFF {
            IdtEntry::new(#i as u16, [<rout_ #i>], 0, 0)
      }"," "]"}),
];

// x86 exceptions

hdl!(div_0, |frame| {
      log::error!("EXCEPTION: Divide by zero");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(debug, |frame| {
      log::error!("EXCEPTION: Debug");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(nmi, |frame| {
      log::error!("EXCEPTION: NMI");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(breakpoint, |frame| {
      log::error!("EXCEPTION: Breakpoint");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(overflow, |frame| {
      log::error!("EXCEPTION: Overflow error");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(bound, |frame| {
      log::error!("EXCEPTION: Bound range exceeded");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(invalid_op, |frame| {
      log::error!("EXCEPTION: Invalid opcode");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(device_na, |frame| {
      log::error!("EXCEPTION: Device not available");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(double_fault, |frame| {
      log::error!("EXCEPTION: Double fault");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(coproc_overrun, |frame| {
      log::error!("EXCEPTION: Coprocessor overrun");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(invalid_tss, |frame| {
      log::error!("EXCEPTION: Invalid TSS");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(segment_na, |frame| {
      log::error!("EXCEPTION: Segment not present");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(stack_fault, |frame| {
      log::error!("EXCEPTION: Stack fault");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(general_prot, |frame| {
      log::error!("EXCEPTION: General protection");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(page_fault, |frame| {
      log::error!("EXCEPTION: Page fault");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC_PF);

      archop::halt_loop(Some(false));
});

hdl!(fp_excep, |frame| {
      log::error!("EXCEPTION: Floating-point exception");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

hdl!(alignment, |frame| {
      log::error!("EXCEPTION: Alignment check");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

// hdl!(mach_check, |frame| {
//       log::error!("EXCEPTION: Machine check");
//       let frame = unsafe { &*frame };
//       frame.dump(Frame::ERRC);

//       archop::halt_loop(Some(false));
// });

hdl!(simd, |frame| {
      log::error!("EXCEPTION: SIMD exception");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
});

// Local APIC interrupts

hdl!(lapic_timer, |frame| {
      crate::cpu::arch::apic::timer::timer_handler(frame);
});

hdl!(lapic_spurious, |_frame| {
      crate::cpu::arch::apic::spurious_handler();
});

hdl!(lapic_error, |_frame| {
      crate::cpu::arch::apic::error_handler();
});

// All other allocable interrupts

extern "C" {
      repeat::repeat! {
            for i in 0x40..0xFF { fn [<rout_ #i>](); }
      }
}

#[no_mangle]
unsafe extern "C" fn common_interrupt(_frame: *mut Frame) {
      archop::halt_loop(Some(false));
}
