use core::ops::Range;

pub use crate::cpu::arch::seg::idt::NR_VECTORS;
use crate::sched::task::ctx::arch::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
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
#[repr(u8)]
pub enum ApicVec {
    Timer = 0x20,
    Error = 0x21,
    IpiTaskMigrate = 0x22,
    Spurious = 0xFF,
}

pub const ALLOC_VEC: Range<u8> = 0x40..0xFF;

type IdtRoute = unsafe extern "C" fn();

macro_rules! single_ent {
    ($vec:expr, $name:ident, $ist:expr, $dpl:expr) => {
        Single(IdtEntry::new(
            $vec as u8,
            paste::paste! {[<rout_ $name>]},
            $ist,
            $dpl,
        ))
    };
}

#[macro_export]
macro_rules! hdl {
    ($name:ident, | $frame_arg:ident | $body:expr) => {
        paste::paste! {
            extern "C" { pub fn [<rout_ $name>](); }
            #[no_mangle]
            unsafe extern "C" fn [<hdl_ $name>]($frame_arg: *const Frame) {
                { $body };
            }
        }
    };
}

pub enum IdtInit {
    Single(IdtEntry),
    Multiple(&'static [IdtEntry]),
}
use IdtInit::*;

pub struct IdtEntry {
    pub vec: u8,
    pub entry: IdtRoute,
    pub ist: u8,
    pub dpl: u16,
}

impl IdtEntry {
    pub const fn new(vec: u8, entry: IdtRoute, ist: u8, dpl: u16) -> Self {
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
    single_ent!(ExVec::Debug, debug, 0, 0),
    single_ent!(ExVec::Nmi, nmi, 0, 0),
    single_ent!(ExVec::Breakpoint, breakpoint, 0, 0),
    single_ent!(ExVec::Overflow, overflow, 0, 3),
    single_ent!(ExVec::Bound, bound, 0, 0),
    single_ent!(ExVec::InvalidOp, invalid_op, 0, 0),
    single_ent!(ExVec::DeviceNa, device_na, 0, 0),
    single_ent!(ExVec::DoubleFault, double_fault, 0, 0),
    single_ent!(ExVec::CoprocOverrun, coproc_overrun, 0, 0),
    single_ent!(ExVec::InvalidTss, invalid_tss, 0, 0),
    single_ent!(ExVec::SegmentNa, segment_na, 0, 0),
    single_ent!(ExVec::StackFault, stack_fault, 0, 0),
    single_ent!(ExVec::GeneralProt, general_prot, 0, 0),
    single_ent!(ExVec::PageFault, page_fault, 0, 0),
    single_ent!(ExVec::FloatPoint, fp_excep, 0, 0),
    single_ent!(ExVec::Alignment, alignment, 0, 0),
    // single_ent!(ExVec::MachineCheck, mach_check, 0, 0),
    single_ent!(ExVec::SimdExcep, simd, 0, 0),
    // Local APIC interrupts
    single_ent!(ApicVec::Timer, lapic_timer, 0, 0),
    single_ent!(ApicVec::Error, lapic_error, 0, 0),
    single_ent!(ApicVec::IpiTaskMigrate, lapic_ipi_task_migrate, 0, 0),
    single_ent!(ApicVec::Spurious, lapic_spurious, 0, 0),
    // All other allocable interrupts
    Multiple(repeat::repeat! {"&[" for i in 0x40..0xFF {
          IdtEntry::new(#i, [<rout_ #i>], 0, 0)
    }"," "]"}),
];

// x86 exceptions

hdl!(div_0, |frame| {
    log::error!("EXCEPTION: Divide by zero");
    super::exception(frame, ExVec::DivideBy0);
});

hdl!(debug, |frame| {
    log::error!("EXCEPTION: Debug");
    super::exception(frame, ExVec::Debug);
});

hdl!(nmi, |frame| {
    log::error!("EXCEPTION: NMI");
    super::exception(frame, ExVec::Nmi);
});

hdl!(breakpoint, |frame| {
    log::error!("EXCEPTION: Breakpoint");
    super::exception(frame, ExVec::Breakpoint);
});

hdl!(overflow, |frame| {
    log::error!("EXCEPTION: Overflow error");
    super::exception(frame, ExVec::Overflow);
});

hdl!(bound, |frame| {
    log::error!("EXCEPTION: Bound range exceeded");
    super::exception(frame, ExVec::Bound);
});

hdl!(invalid_op, |frame| {
    log::error!("EXCEPTION: Invalid opcode");
    super::exception(frame, ExVec::InvalidOp);
});

hdl!(device_na, |frame| {
    log::error!("EXCEPTION: Device not available");
    super::exception(frame, ExVec::DeviceNa);
});

hdl!(double_fault, |frame| {
    log::error!("EXCEPTION: Double fault");
    super::exception(frame, ExVec::DoubleFault);
});

hdl!(coproc_overrun, |frame| {
    log::error!("EXCEPTION: Coprocessor overrun");
    super::exception(frame, ExVec::CoprocOverrun);
});

hdl!(invalid_tss, |frame| {
    log::error!("EXCEPTION: Invalid TSS");
    super::exception(frame, ExVec::InvalidTss);
});

hdl!(segment_na, |frame| {
    log::error!("EXCEPTION: Segment not present");
    super::exception(frame, ExVec::SegmentNa);
});

hdl!(stack_fault, |frame| {
    log::error!("EXCEPTION: Stack fault");
    super::exception(frame, ExVec::StackFault);
});

hdl!(general_prot, |frame| {
    log::error!("EXCEPTION: General protection");
    super::exception(frame, ExVec::GeneralProt);
});

hdl!(page_fault, |frame| {
    super::exception(frame, ExVec::PageFault);
});

hdl!(fp_excep, |frame| {
    log::error!("EXCEPTION: Floating-point exception");
    super::exception(frame, ExVec::FloatPoint);
});

hdl!(alignment, |frame| {
    log::error!("EXCEPTION: Alignment check");
    super::exception(frame, ExVec::Alignment);
});

// hdl!(mach_check, |frame| {
//       log::error!("EXCEPTION: Machine check");
//       let frame = unsafe { &*frame };
//       frame.dump(Frame::ERRC);

//       archop::halt_loop(Some(false));
// });

hdl!(simd, |frame| {
    log::error!("EXCEPTION: SIMD exception");
    super::exception(frame, ExVec::SimdExcep);
});

// Local APIC interrupts

hdl!(lapic_timer, |_frame| {
    crate::cpu::arch::apic::timer::timer_handler();
});

hdl!(lapic_error, |_frame| {
    crate::cpu::arch::apic::error_handler();
});

hdl!(lapic_ipi_task_migrate, |_frame| {
    crate::sched::task_migrate_handler();
});

hdl!(lapic_spurious, |_frame| {
    crate::cpu::arch::apic::spurious_handler();
});

// All other allocable interrupts

extern "C" {
    repeat::repeat! {
          for i in 0x40..0xFF { fn [<rout_ #i>](); }
    }
}
