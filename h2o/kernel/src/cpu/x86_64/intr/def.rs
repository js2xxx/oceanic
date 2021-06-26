use super::ctx::Frame;

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

type IdtRoute = unsafe extern "C" fn();

pub struct IdtInit {
      pub vec: ExVec,
      pub entry: IdtRoute,
      pub ist: u8,
      pub dpl: u16,
}

impl IdtInit {
      pub const fn new(vec: ExVec, entry: IdtRoute, ist: u8, dpl: u16) -> Self {
            IdtInit {
                  vec,
                  entry,
                  ist,
                  dpl,
            }
      }
}

pub static IDT_INIT: &[IdtInit] = &[
      IdtInit::new(ExVec::DivideBy0, intr_gen::rout!(div_0), 0, 0),
      IdtInit::new(ExVec::Overflow, intr_gen::rout!(overflow), 0, 0),
      IdtInit::new(ExVec::CoprocOverrun, intr_gen::rout!(coproc_overrun), 0, 0),
      IdtInit::new(ExVec::InvalidTss, intr_gen::rout!(invalid_tss), 0, 0),
      IdtInit::new(ExVec::SegmentNa, intr_gen::rout!(segment_na), 0, 0),
      IdtInit::new(ExVec::StackFault, intr_gen::rout!(stack_fault), 0, 0),
];

#[intr_gen::hdl]
unsafe fn div_0(frame: *mut Frame) {
      log::error!("EXCEPTION: Divide by zero");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
}

#[intr_gen::hdl]
unsafe fn overflow(frame: *mut Frame) {
      log::error!("EXCEPTION: Overflow error");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
}

#[intr_gen::hdl]
unsafe fn coproc_overrun(frame: *mut Frame) {
      log::error!("EXCEPTION: Coprocessor overrun");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
}

#[intr_gen::hdl]
unsafe fn invalid_tss(frame: *mut Frame) {
      log::error!("EXCEPTION: Invalid TSS");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
}

#[intr_gen::hdl]
unsafe fn segment_na(frame: *mut Frame) {
      log::error!("EXCEPTION: Segment not present");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
}

#[intr_gen::hdl]
unsafe fn stack_fault(frame: *mut Frame) {
      log::error!("EXCEPTION: Stack fault");
      let frame = unsafe { &*frame };
      frame.dump(Frame::ERRC);

      archop::halt_loop(Some(false));
}
