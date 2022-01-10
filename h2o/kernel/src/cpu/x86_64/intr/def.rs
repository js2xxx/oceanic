use core::{fmt, ops::Range};

pub use crate::cpu::arch::seg::idt::NR_VECTORS;
use crate::sched::task::ctx::arch::Frame;

#[derive(Clone, Copy, PartialEq, Eq)]
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

impl fmt::Debug for ExVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DivideBy0 => write!(f, "Divide by zero"),
            Self::Debug => write!(f, "Debug"),
            Self::Nmi => write!(f, "Non-maskable interrupt"),
            Self::Breakpoint => write!(f, "Breakpoint"),
            Self::Overflow => write!(f, "Overflow"),
            Self::Bound => write!(f, "Bound range exceeded"),
            Self::InvalidOp => write!(f, "Invalid opcode"),
            Self::DeviceNa => write!(f, "Device not available"),
            Self::DoubleFault => write!(f, "Double fault"),
            Self::CoprocOverrun => write!(f, "Coprocessor overrun"),
            Self::InvalidTss => write!(f, "Invalid TSS"),
            Self::SegmentNa => write!(f, "Segment not available"),
            Self::StackFault => write!(f, "Stack fault"),
            Self::GeneralProt => write!(f, "General protection"),
            Self::PageFault => write!(f, "Page fault"),
            Self::FloatPoint => write!(f, "Floatpoint exception"),
            Self::Alignment => write!(f, "Alignment check"),
            Self::MachineCheck => write!(f, "Machine check"),
            Self::SimdExcep => write!(f, "SIMD exception"),
        }
    }
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
            unsafe extern "C" fn [<hdl_ $name>]($frame_arg: *mut Frame) {
                { $body };
            }
        }
    };

    ($($name:ident => $vec:tt),*) => {
        use ExVec::*;
        $(hdl!($name, |frame| {
            super::exception(frame, $vec);
        });)*
    }
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
hdl!(
    div_0 => DivideBy0,
    debug => Debug,
    nmi => Nmi,
    breakpoint => Breakpoint,
    overflow => Overflow,
    bound => Bound,
    invalid_op => InvalidOp,
    device_na => DeviceNa,
    double_fault => DoubleFault,
    coproc_overrun => CoprocOverrun,
    invalid_tss => InvalidTss,
    segment_na => SegmentNa,
    stack_fault => StackFault,
    general_prot => GeneralProt,
    page_fault => PageFault,
    fp_excep => FloatPoint,
    alignment => Alignment,
    simd => SimdExcep
);

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
