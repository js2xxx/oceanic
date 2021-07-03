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
      Virtual = 0x14,
      ControlProt = 0x15,
      VmmComm = 0x1D,
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

macro_rules! rout {
      ($name:ident) => {
            paste::paste! { [<rout_ $name>] }
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
      S(IdtEntry),
      M(&'static [IdtEntry]),
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
            IdtEntry { vec, entry, ist, dpl }
      }
}

pub static IDT_INIT: &[IdtInit] = &[
      S(IdtEntry::new(ExVec::DivideBy0 as u16, rout!(div_0), 0, 0)),
      S(IdtEntry::new(ExVec::Debug as u16, rout!(debug), 0, 0)),
      S(IdtEntry::new(ExVec::Nmi as u16, rout!(nmi), 0, 0)),
      S(IdtEntry::new(ExVec::Breakpoint as u16, rout!(breakpoint), 0, 0)),
      S(IdtEntry::new(ExVec::Overflow as u16, rout!(overflow), 0, 3)),
      S(IdtEntry::new(ExVec::Bound as u16, rout!(bound), 0, 0)),
      S(IdtEntry::new(ExVec::InvalidOp as u16, rout!(invalid_op), 0, 0)),
      S(IdtEntry::new(ExVec::DeviceNa as u16, rout!(device_na), 0, 0)),
      S(IdtEntry::new(ExVec::DoubleFault as u16, rout!(double_fault), 0, 0)),
      S(IdtEntry::new(ExVec::CoprocOverrun as u16, rout!(coproc_overrun), 0, 0)),
      S(IdtEntry::new(ExVec::InvalidTss as u16, rout!(invalid_tss), 0, 0)),
      S(IdtEntry::new(ExVec::SegmentNa as u16, rout!(segment_na), 0, 0)),
      S(IdtEntry::new(ExVec::StackFault as u16, rout!(stack_fault), 0, 0)),
      S(IdtEntry::new(ExVec::GeneralProt as u16, rout!(general_prot), 0, 0)),
      S(IdtEntry::new(ExVec::PageFault as u16, rout!(page_fault), 0, 0)),
      M(repeat::repeat! {"&[" for i in 0x40..0xFF {
            IdtEntry::new(#i as u16, [<rout_ #i>], 0, 0)
      }"," "]"}),
];

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

extern "C" {
      repeat::repeat! {
            for i in 0x40..0xFF { fn [<rout_ #i>](); }
      }
}

#[no_mangle]
unsafe extern "C" fn common_interrupt(frame: *mut Frame) {
      archop::halt_loop(Some(false));
}
