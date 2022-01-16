pub mod alloc;
pub(super) mod def;

use ::alloc::sync::{Arc, Weak};
use spin::Mutex;

pub use self::def::ExVec;
use self::def::NR_VECTORS;
use crate::{
    cpu::{
        arch::{apic::lapic, seg::ndt::USR_CODE_X64},
        intr::Interrupt,
        time::Instant,
    },
    sched::{
        task::{self, ctx::arch::Frame},
        SCHED,
    },
};

#[allow(clippy::declare_interior_mutable_const)]
const VEC_INTR_INIT: Mutex<Option<Weak<Interrupt>>> = Mutex::new(None);
#[thread_local]
static VEC_INTR: [Mutex<Option<Weak<Interrupt>>>; NR_VECTORS] = [VEC_INTR_INIT; NR_VECTORS];

#[derive(Debug, Clone)]
pub struct ArchReg {
    vec: u8,
    cpu: usize,
}

impl ArchReg {
    pub fn vector(&self) -> u8 {
        self.vec
    }

    pub fn cpu(&self) -> usize {
        self.cpu
    }
}

#[derive(Debug)]
pub enum RegisterError {
    NotCurCpu,
    Pending,
}

/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure
/// to make preparations.
pub unsafe fn try_register(
    intr: &Arc<Interrupt>,
) -> Result<Option<Weak<Interrupt>>, RegisterError> {
    let ArchReg { ref vec, ref cpu } = &*intr.arch_reg().lock();
    if *cpu != crate::cpu::id() {
        return Err(RegisterError::NotCurCpu);
    }

    if let Some(mut intr_slot) = VEC_INTR[*vec as usize].try_lock() {
        Ok(intr_slot.replace(Arc::downgrade(intr)))
    } else {
        Err(RegisterError::Pending)
    }
}

/// # Safety
///
/// WARNING: This function modifies the architecture's basic registers. Be sure
/// to make preparations.
pub unsafe fn try_unregister(intr: &Arc<Interrupt>) -> Result<(), RegisterError> {
    let ArchReg { ref vec, ref cpu } = &*intr.arch_reg().lock();
    if *cpu != crate::cpu::id() {
        return Err(RegisterError::NotCurCpu);
    }

    if let Some(mut intr_slot) = VEC_INTR[*vec as usize].try_lock() {
        intr_slot.replace(Weak::new());
        Ok(())
    } else {
        Err(RegisterError::Pending)
    }
}

/// Generic exception handler.
unsafe fn exception(frame_ptr: *mut Frame, vec: def::ExVec) {
    use def::ExVec::*;

    let frame = &mut *frame_ptr;
    match vec {
        PageFault if crate::mem::space::page_fault(&mut *frame_ptr, frame.errc_vec) => return,
        _ => {}
    }

    match SCHED.with_current(|cur| cur.tid().ty()) {
        Some(task::Type::User) if frame.cs == USR_CODE_X64.into_val().into() => {
            if !task::dispatch_exception(frame, vec) {
                // Kill the fucking task.
                SCHED.exit_current((-solvent::EFAULT) as usize)
            }
            // unreachable!()
        }
        _ => {}
    }

    // No more available remedies. Die.
    log::error!("{:?}", vec);

    frame.dump(if vec == PageFault {
        Frame::ERRC_PF
    } else {
        Frame::ERRC
    });

    archop::halt_loop(Some(false));
}

/// # Safety
///
/// This function must only be called from its assembly routine `rout_XX`.
#[no_mangle]
unsafe extern "C" fn common_interrupt(frame: *mut Frame) {
    let vec = unsafe { &*frame }.errc_vec as u16;
    if let Some(mut intr_slot) = VEC_INTR[vec as usize].try_lock() {
        if let Some(intr) = intr_slot.clone().and_then(|intr_weak| {
            intr_weak.upgrade().or_else(|| {
                // Automatically unregister the interrupt weak link.
                let _ = intr_slot.take();
                None
            })
        }) {
            intr.handle();
        } else {
            lapic(|lapic| lapic.eoi());

            log::warn!("No interrupt for vector {:#x}", vec);
        }
    } else {
        log::warn!(
            "The interrupt for vector {:#x} is already firing without blocking next ones",
            vec
        );
    }
    crate::sched::SCHED.tick(Instant::now());
}
