use super::ctx::Frame;

use core::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum IntrVec {
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
      InvalidTss = 10,
      SegmentNa = 11,
      StackFault = 12,
      GeneralProt = 13,
      PageFault = 14,
      Spurious = 15,
      FloatPoint = 16,
      Alignment = 17,
      MachineCheck = 18,
      SimdExcep = 19,
      Virtual = 20,
      ControlProt = 21,
      VmmComm = 29,
}

pub const ALLOC_VEC: Range<u16> = 0x40..0xFF;

type IdtRoute = unsafe extern "C" fn();

macro_rules! rout {
      ($name:ident) => {
            paste::paste! { [<rout_ $name>] }
      };
}

macro_rules! hdl {
      ($name:ident, |$frame_arg:ident| $body:block) => {
            paste::paste! {
                  extern "C" { fn [<rout_ $name>](); }
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
      S(IdtEntry::new(IntrVec::DivideBy0 as u16, rout!(div_0), 0, 0)),
      S(IdtEntry::new(IntrVec::Overflow as u16, rout!(overflow), 0, 0)),
      S(IdtEntry::new(IntrVec::CoprocOverrun as u16, rout!(coproc_overrun), 0, 0)),
      S(IdtEntry::new(IntrVec::InvalidTss as u16, rout!(invalid_tss), 0, 0)),
      S(IdtEntry::new(IntrVec::SegmentNa as u16, rout!(segment_na), 0, 0)),
      S(IdtEntry::new(IntrVec::StackFault as u16, rout!(stack_fault), 0, 0)),
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

hdl!(overflow, |frame| {
      log::error!("EXCEPTION: Overflow error");
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

extern "C" {
      repeat::repeat! {
            for i in 0x40..0xFF { fn [<rout_ #i>](); }
      }
}

#[no_mangle]
unsafe extern "C" fn common_interrupt(frame: *mut Frame) {
      archop::halt_loop(Some(false));
}
