pub mod alloc;

use core::sync::atomic::{AtomicU16, Ordering};

use ::alloc::{sync::Arc, vec::Vec};
use bitop_ex::BitOpEx;
use spin::{Lazy, Mutex};

use self::arch::ArchReg;
pub use super::arch::intr as arch;

pub static ALLOC: Lazy<Mutex<alloc::Allocator>> =
    Lazy::new(|| Mutex::new(alloc::Allocator::new(crate::cpu::count())));

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum IsaIrq {
    Pit = 0,
    Ps2Keyboard = 1,
    Pic2 = 2,
    Serial2 = 3,
    Serial1 = 4,
    Printer1 = 7,
    Rtc = 8,
    Ps2Mouse = 12,
    Ide0 = 14,
    Ide1 = 15,
}

bitflags::bitflags! {
    pub struct IrqReturn: u8 {
        const SUCCESS = 0b0000_0001;
        const WAKE_TASK = 0b0000_0010;
        const DISABLED = 0b0000_0100;
        const UNMASK = 0b0000_1000;
    }
}

pub type TypeHandler = unsafe fn(Arc<Interrupt>);

pub struct Handler {
    func: fn(*mut u8) -> IrqReturn,
    arg: *mut u8,
}

impl Handler {
    pub fn new(func: fn(*mut u8) -> IrqReturn, arg: *mut u8) -> Self {
        Handler { func, arg }
    }
}

pub trait IntrChip: Send + Sync {
    /// Set up a interrupt in the chip.
    ///
    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    unsafe fn setup(&mut self, arch_reg: ArchReg, gsi: u32) -> Result<TypeHandler, &'static str>;

    /// Remove a interrupt from the chip.
    ///
    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    unsafe fn remove(&mut self, intr: Arc<Interrupt>) -> Result<(), &'static str>;

    /// Mask a interrupt so as to forbid it from triggering.
    ///
    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    unsafe fn mask(&mut self, intr: Arc<Interrupt>);

    /// Unmask a interrupt so that it can trigger.
    ///
    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    unsafe fn unmask(&mut self, intr: Arc<Interrupt>);

    /// Acknowledge a interrupt in the beginning of its handler.
    ///
    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    unsafe fn ack(&mut self, intr: Arc<Interrupt>);

    /// Mask and acknowledge a interrupt in the beginning of its handler.
    ///
    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    unsafe fn mask_ack(&mut self, intr: Arc<Interrupt>) {
        self.mask(intr.clone());
        self.ack(intr);
    }

    /// Mark the end of the interrupt's handler.
    ///
    /// # Safety
    ///
    /// WARNING: This function modifies the architecture's basic registers. Be
    /// sure to make preparations.
    unsafe fn eoi(&mut self, intr: Arc<Interrupt>);
}

pub struct State;
impl State {
    pub const ENABLED: u16 = 0b0000_0001;
    pub const ONESHOT: u16 = 0b0000_0010;
    pub const RUNNING: u16 = 0b0000_0100;
    pub const PENDING: u16 = 0b0000_1000;
}

pub struct Interrupt {
    state: AtomicU16,
    gsi: u32,
    hw_irq: u8,
    chip: Arc<Mutex<dyn IntrChip>>,
    arch_reg: Mutex<arch::ArchReg>,
    type_handler: TypeHandler,
    affinity: super::CpuMask,
    handlers: Mutex<Vec<Handler>>,
}

unsafe impl Send for Interrupt {}
unsafe impl Sync for Interrupt {}

impl Interrupt {
    pub fn gsi(&self) -> u32 {
        self.gsi
    }

    pub fn hw_irq(&self) -> u8 {
        self.hw_irq
    }

    pub fn chip(&self) -> Arc<Mutex<dyn IntrChip>> {
        self.chip.clone()
    }

    pub fn arch_reg(&self) -> &Mutex<arch::ArchReg> {
        &self.arch_reg
    }

    pub(super) unsafe fn handle(self: &Arc<Interrupt>) {
        (self.type_handler)(self.clone())
    }

    pub fn affinity(&self) -> &super::CpuMask {
        &self.affinity
    }

    pub fn handlers(&self) -> &Mutex<Vec<Handler>> {
        &self.handlers
    }
}

fn handle_event(intr: Arc<Interrupt>) -> IrqReturn {
    let state = intr.state.load(Ordering::SeqCst);
    if state.contains_bit(State::ENABLED) {
        let mut ret = IrqReturn::empty();
        for Handler { func, arg } in intr.handlers.lock().iter() {
            let r = (func)(*arg);
            // TODO: wake up tasks if specified.
            ret |= r;
        }
        if intr
            .state
            .load(Ordering::SeqCst)
            .contains_bit(State::ONESHOT)
        {
            ret
        } else {
            ret | IrqReturn::UNMASK
        }
    } else {
        intr.state.store(state | State::PENDING, Ordering::SeqCst);
        IrqReturn::DISABLED
    }
}

/// Handle a EDGE-triggered interrupt from the current interrupt handler.
///
/// # Safety
///
/// This function must be called only from the interrupt handler.
pub unsafe fn level_handler(intr: Arc<Interrupt>) {
    intr.chip.lock().mask_ack(intr.clone());
    let ret = handle_event(intr.clone());
    if !ret.contains(IrqReturn::DISABLED) && ret.contains(IrqReturn::UNMASK) {
        intr.chip.lock().unmask(intr.clone());
    }
}

/// Handle a fast EOI-triggered interrupt from the current interrupt handler.
///
/// # Safety
///
/// This function must be called only from the interrupt handler.
pub unsafe fn fasteoi_handler(intr: Arc<Interrupt>) {
    let ret = handle_event(intr.clone());
    if !ret.contains(IrqReturn::DISABLED) {
        let mut chip = intr.chip.lock();
        chip.eoi(intr.clone());
        if ret.contains(IrqReturn::UNMASK) {
            chip.unmask(intr.clone());
        }
    }
}

/// Handle a EDGE-triggered interrupt from the current interrupt handler.
///
/// # Safety
///
/// This function must be called only from the interrupt handler.
pub unsafe fn edge_handler(intr: Arc<Interrupt>) {
    let state = intr.state.fetch_or(State::RUNNING, Ordering::SeqCst);
    if state.contains_bit(State::RUNNING) {
        intr.chip.lock().mask_ack(intr.clone());
        intr.state.fetch_or(State::PENDING, Ordering::SeqCst);
        return;
    }

    intr.chip.lock().ack(intr.clone());

    while {
        let state = intr.state.load(Ordering::SeqCst);
        state.contains_bit(State::PENDING) && state.contains_bit(State::ENABLED)
    } {
        intr.chip.lock().unmask(intr.clone());

        intr.state.fetch_and(!State::PENDING, Ordering::SeqCst);
        let ret = handle_event(intr.clone());
        if ret.contains(IrqReturn::DISABLED) {
            break;
        }
    }
    intr.state.fetch_and(!State::RUNNING, Ordering::SeqCst);
}
