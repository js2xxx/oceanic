use super::ctx::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ExVector {
      DivideBy0,
      Debug,
      Nmi,
      Breakpoint,
      Overflow,
      Bound,
      InvalidOp,
      DeviceNa,
      DoubleFault,
      CoprocOverrun,
      InvalidTss,
      SegmentNa,
      StackFault,
      GeneralProt,
      PageFault,
      Spurious,
      FloatPoint,
      Alignment,
      MachineCheck,
      SimdExcep,
      Virtual,
      ControlProt,
      VmmComm = 29,
}

type IdtRoute = unsafe extern "C" fn();

pub struct IdtInit {
      pub vec: ExVector,
      pub entry: IdtRoute,
      pub ist: u8,
      pub dpl: u16,
}

impl IdtInit {
      pub const fn new(vec: ExVector, entry: IdtRoute, ist: u8, dpl: u16) -> Self {
            IdtInit {
                  vec,
                  entry,
                  ist,
                  dpl,
            }
      }
}

pub static IDT_INIT: &[IdtInit] = &[
      IdtInit::new(ExVector::DivideBy0, intr_gen::rout!(div_0), 0, 0),
      IdtInit::new(ExVector::Overflow, intr_gen::rout!(overflow), 0, 0),
      IdtInit::new(ExVector::CoprocOverrun, intr_gen::rout!(coproc_overrun), 0, 0),
      IdtInit::new(ExVector::InvalidTss, intr_gen::rout!(invalid_tss), 0, 0),
      IdtInit::new(ExVector::SegmentNa, intr_gen::rout!(segment_na), 0, 0),
      IdtInit::new(ExVector::StackFault, intr_gen::rout!(stack_fault), 0, 0),
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
      log::error!("EXCEPTION: Coprocessor overrun");
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