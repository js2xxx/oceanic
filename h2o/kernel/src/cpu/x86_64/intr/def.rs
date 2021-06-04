//! TODO: Write a macro to define interrupt entries and to define the initial IDT.
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

pub static IDT_INIT: &[IdtInit] = &[IdtInit::new(ExVector::DivideBy0, rout_div_0, 0, 0)];

macro_rules! define_intr {
      {$vec:expr, $asm_name:ident, $name:ident, $body:block} => {
            extern "C" {
                  pub fn $asm_name();
            }

            #[no_mangle]
            pub extern "C" fn $name(frame: *mut Frame) $body
      }
}

// define_intr! {1, rout_dummy, hdl_dummy, {
//       let a = 1;
// }}
define_intr! {0, rout_div_0, hdl_div_0, {
      log::error!("Divide by zero");
      loop {
            unsafe { asm!("pause")};
      }
}}
