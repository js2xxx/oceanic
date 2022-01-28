use core::{arch::asm, cell::UnsafeCell, mem::size_of, ops::Deref, ptr::addr_of};

use archop::Azy;
use bitvec::BitArr;
use paging::LAddr;
use static_assertions::*;

use super::*;
use crate::cpu::CpuLocalLazy;

pub const KRL_CODE_X64: SegSelector = SegSelector::from_const(0x08); // SegSelector::new().with_index(1)
pub const KRL_DATA_X64: SegSelector = SegSelector::from_const(0x10); // SegSelector::new().with_index(2)
pub const USR_CODE_X86: SegSelector = SegSelector::from_const(0x18); // SegSelector::new().with_index(3)
pub const USR_DATA_X64: SegSelector = SegSelector::from_const(0x20 + 3); // SegSelector::new().with_index(4).with_rpl(3)
pub const USR_CODE_X64: SegSelector = SegSelector::from_const(0x28 + 3); // SegSelector::new().with_index(5).with_rpl(3)

pub const GDT_LDTR: SegSelector = SegSelector::from_const(0x30); // SegSelector::new().with_index(6)

pub const GDT_TR: SegSelector = SegSelector::from_const(0x40); // SegSelector::new().with_index(8)

pub const INTR_CODE: SegSelector = SegSelector::from_const(0x08 + 4); // SegSelector::new().with_index(1).with_ti(true)
pub const INTR_DATA: SegSelector = SegSelector::from_const(0x10 + 4); // SegSelector::new().with_index(2).with_ti(true)

const INIT_LIM: u32 = 0xFFFFF;
const INIT_ATTR: u16 = attrs::PRESENT | attrs::G4K;

// NOTE: The segment tables must be initialized in `Azy` or mutable statics.
// Otherwise the compiler or the linker will place it into the constant section
// of the executable file and cause load errors.

static LDT: Azy<DescTable<3>> = Azy::new(|| {
    DescTable::new([
        Segment::new(0, 0, 0, 0),
        Segment::new(0, INIT_LIM, attrs::SEG_CODE | attrs::X64 | INIT_ATTR, 0),
        Segment::new(0, INIT_LIM, attrs::SEG_DATA | attrs::X64 | INIT_ATTR, 0),
    ])
});

#[thread_local]
pub static GDT: CpuLocalLazy<DescTable<10>> = CpuLocalLazy::new(|| {
    DescTable::new([
        Segment::new(0, 0, 0, 0),
        Segment::new(0, INIT_LIM, attrs::SEG_CODE | attrs::X64 | INIT_ATTR, 0),
        Segment::new(0, INIT_LIM, attrs::SEG_DATA | attrs::X64 | INIT_ATTR, 0),
        Segment::new(0, INIT_LIM, attrs::SEG_CODE | attrs::X86 | INIT_ATTR, 0),
        Segment::new(0, INIT_LIM, attrs::SEG_DATA | attrs::X64 | INIT_ATTR, 3),
        Segment::new(0, INIT_LIM, attrs::SEG_CODE | attrs::X64 | INIT_ATTR, 3),
        Segment::new_fp(LDT.export_fp(), attrs::SYS_LDT | attrs::PRESENT, 0),
        unsafe { Segment::new_fp_high(LDT.export_fp()) },
        Segment::new_fp(TSS.export_fp(), attrs::SYS_TSS | attrs::PRESENT, 0),
        unsafe { Segment::new_fp_high(TSS.export_fp()) },
    ])
});

#[thread_local]
pub(in crate::cpu::arch) static TSS: CpuLocalLazy<Tss> = CpuLocalLazy::new(|| {
    // SAFETY: No physical address specified.
    let alloc_stack = || {
        crate::mem::alloc_system_stack()
            .expect("System memory allocation failed")
            .as_ptr()
    };

    let rsp0 = alloc_stack();
    let ist1 = alloc_stack();

    Tss {
        data: TssStruct {
            // The legacy RSPs of different privilege levels.
            rsp0: UnsafeCell::new(rsp0 as u64),
            // The Interrupt Stack Tables.
            ist: [ist1 as u64, 0, 0, 0, 0, 0, 0],
            ..Default::default()
        },
        ..Default::default()
    }
});

unsafe impl Sync for TssStruct {}

/// All the segment descriptor that consumes a quadword.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    attr_low: u8,
    attr_high_limit_high: u8,
    base_high: u8,
}
const_assert_eq!(size_of::<Segment>(), size_of::<u64>());

/// The Task State Segment.
#[derive(Default)]
#[repr(C, packed)]
pub struct TssStruct {
    _rsvd1: u32,
    /// The legacy RSPs of different privilege levels.
    rsp0: UnsafeCell<u64>,
    rsp1: u64,
    rsp2: u64,
    _rsvd2: u64,
    /// The Interrupt Stack Tables.
    ist: [u64; 7],
    _rsvd3: u64,
    _rsvd4: u16,
    /// The IO base mappings.
    io_base: UnsafeCell<u16>,
}

impl TssStruct {
    pub unsafe fn rsp0(&self) -> u64 {
        let addr = addr_of!(self.rsp0);
        addr.read_unaligned().into_inner()
    }

    pub unsafe fn set_rsp0(&self, rsp0: u64) -> u64 {
        let ret = self.rsp0();
        let addr = addr_of!(self.rsp0);
        (addr as *mut u64).write_unaligned(rsp0);
        ret
    }

    unsafe fn set_io_base(&self, offset: u16) {
        let addr = addr_of!(self.io_base);
        (addr as *mut u16).write_unaligned(offset);
    }
}

#[derive(Default)]
#[repr(C)]
pub struct Tss {
    data: TssStruct,
    bitmap: UnsafeCell<BitArr!(for 65536 + 8)>,
}

impl Tss {
    #[inline]
    pub fn bitmap(&self) -> *mut BitArr!(for 65536) {
        self.bitmap.get() as *mut BitArr!(for 65536)
    }

    pub fn export_fp(&self) -> FatPointer {
        FatPointer {
            base: LAddr::new(self as *const _ as *mut _),
            limit: size_of::<Self>() as u16 - 1,
        }
    }
}

impl Deref for Tss {
    type Target = TssStruct;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// A descriptor table.
#[derive(Debug)]
#[repr(align(16))]
pub struct DescTable<const N: usize>([Segment; N]);

impl<const N: usize> DescTable<N> {
    /// Construct a new descriptor table.
    pub const fn new(array: [Segment; N]) -> Self {
        DescTable(array)
    }

    pub const fn size(&self) -> usize {
        self.0.len() * size_of::<Segment>()
    }

    /// Export the fat pointer of the descriptor table.
    pub fn export_fp(&self) -> FatPointer {
        FatPointer {
            base: LAddr::new(self.0.as_ptr() as *mut _),
            limit: self.size() as u16 - 1,
        }
    }
}

impl Segment {
    /// Create a new segment descriptor.
    ///
    /// The caller must ensure that the limit or the Descriptor Privilege Level
    /// is within the available range.
    pub const fn new(base: u32, limit: u32, attr: u16, dpl: u16) -> Self {
        Segment {
            limit_low: (limit & 0xFFFF) as _,
            base_low: (base & 0xFFFF) as _,
            base_mid: ((base >> 16) & 0xFF) as _,
            attr_low: ((attr & 0xFF) | ((dpl & 3) << 5)) as _,
            attr_high_limit_high: ((limit >> 16) & 0xF) as u8 | ((attr >> 8) & 0xF0) as u8,
            base_high: ((base >> 24) & 0xFF) as _,
        }
    }

    /// Create a new system descriptor according to a [`FatPointer`].
    ///
    /// The caller must ensure that the limit or the Descriptor Privilege Level
    /// is within the available range.
    pub fn new_fp(fp: FatPointer, attr: u16, dpl: u16) -> Self {
        Self::new(
            (fp.base.val() & 0xFFFFFFFF) as u32,
            fp.limit as u32,
            attr,
            dpl,
        )
    }

    /// Create the higher half of a segment descriptor.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the entry before it is a valid system
    /// segment descriptor.
    pub const unsafe fn new_high(base_high: u32) -> Self {
        core::mem::transmute(base_high as u64)
    }

    /// # Safety
    ///
    /// See [`new_high`] for more details.
    pub unsafe fn new_fp_high(fp: FatPointer) -> Self {
        Self::new_high((fp.base.val() >> 32) as u32)
    }
}

/// Load a GDT into x86 architecture's `gdtr` and reset all the segment
/// registers according to it.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure
/// to make preparations.
///
/// The caller must ensure that `gdt` is a valid GDT object and `krl_sel`
/// consists of the kernel's code & data selector in `gdt`.
unsafe fn load_gdt() {
    extern "C" {
        fn reset_seg(code: SegSelector, data: SegSelector);
    }

    let gdtr = GDT.export_fp();
    asm!("lgdt [{}]", in(reg) &gdtr);

    reset_seg(KRL_CODE_X64, KRL_DATA_X64);
}

/// Load an LDT into x86 architecture's `ldtr`.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure
/// to make preparations.
///
/// The caller must ensure that `ldtr` points to a valid LDT and its GDT is
/// loaded.
unsafe fn load_ldt(ldtr: SegSelector) {
    asm!("lldt [{}]", in(reg) &ldtr);
}

/// Load an TSS into x86 architecture's `tr`.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure
/// to make preparations.
///
/// The caller must ensure that `tr` points to a valid TSS and its GDT is
/// loaded.
unsafe fn load_tss(tr: SegSelector) {
    let tss_addr = &TSS.data as *const TssStruct as *const u8;
    let map_addr = &TSS.bitmap as *const UnsafeCell<_> as *const u8;
    let io_base = u16::try_from(map_addr.offset_from(tss_addr)).expect("Failed to get io_base");
    TSS.set_io_base(io_base);

    let last_byte = map_addr.add(65536 / 8);
    (last_byte as *mut u8).write(u8::MAX);

    unsafe { asm!("ltr [{}]", in(reg) &tr) };
}

/// Initialize NDT (GDT & LDT & TSS) in x86 architecture by the bootstrap CPU.
///
/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure
/// to make preparations.
///
/// The caller must ensure that this function is called only once from the
/// bootstrap CPU.
pub unsafe fn init() {
    unsafe {
        load_gdt();
        load_ldt(GDT_LDTR);
        load_tss(GDT_TR);
    }
}
