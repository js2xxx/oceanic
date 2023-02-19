pub(super) mod def;

use alloc::vec::Vec;
use core::{
    iter,
    ops::Range,
    sync::atomic::{AtomicUsize, Ordering},
};

use archop::Azy;
use array_macro::array;
use bitvec::{bitbox, prelude::BitBox};
use spin::Mutex;
use sv_call::res::Msi;

pub use self::def::{ExVec, ALLOC_VEC};
use super::apic::LAPIC_ID;
use crate::{
    cpu::{
        arch::seg::ndt::USR_CODE_X64,
        intr::{Interrupt, IntrHandler},
        time::Instant,
    },
    mem::space::PageFaultErrCode,
    sched::{
        task::{self, ctx::arch::Frame},
        PREEMPT, SCHED,
    },
};

static MANAGER: Azy<Vec<Manager>> = Azy::new(|| {
    iter::repeat_with(Default::default)
        .take(crate::cpu::count())
        .collect()
});
const ALLOC_VEC_INDEX: Range<usize> = (ALLOC_VEC.start as usize)..(ALLOC_VEC.end as usize);

pub struct Manager {
    map: Mutex<BitBox>,
    slots: [Mutex<Option<(IntrHandler, *const Interrupt)>>; u8::MAX as usize + 1],
    count: AtomicUsize,
}

unsafe impl Sync for Manager {}
unsafe impl Send for Manager {}

impl Manager {
    pub fn new() -> Self {
        Manager {
            map: Mutex::new(bitbox![0; 256]),
            slots: array![_ => Mutex::new(None); 256],
            count: AtomicUsize::new(0),
        }
    }

    pub fn invoke(vec: u8) {
        PREEMPT.scope(|| {
            let manager = &MANAGER[unsafe { crate::cpu::id() }];
            if let Some((handler, arg)) = *manager.slots[vec as usize].lock() {
                handler(arg);
            } else {
                log::trace!("Unhandled interrupt #{:?}", vec);
            }
        })
    }

    pub fn select_cpu() -> usize {
        MANAGER
            .iter()
            .enumerate()
            .fold((usize::MAX, usize::MAX), |(acc, iacc), (index, manager)| {
                let value = manager.count.load(Ordering::Acquire);
                if value < acc {
                    (value, index)
                } else {
                    (acc, iacc)
                }
            })
            .1
    }

    pub fn register(cpu: usize, handler: (IntrHandler, *const Interrupt)) -> sv_call::Result<u8> {
        let _pree = PREEMPT.lock();

        let manager = MANAGER.get(cpu).ok_or(sv_call::ENODEV)?;

        let vec = {
            let mut map = manager.map.lock();
            let index = map[ALLOC_VEC_INDEX].first_zero().ok_or(sv_call::ENOSPC)?;
            let vec = index + ALLOC_VEC_INDEX.start;
            map.set(vec, true);
            manager.count.fetch_add(1, Ordering::SeqCst);
            vec as u8
        };

        *manager.slots[vec as usize].lock() = Some(handler);

        Ok(vec)
    }

    pub fn register_handler(
        cpu: usize,
        vec: u8,
        handler: (fn(*const Interrupt), *const Interrupt),
    ) -> sv_call::Result {
        let manager = MANAGER.get(cpu).ok_or(sv_call::ENODEV)?;

        *manager.slots[vec as usize].lock() = Some(handler);
        Ok(())
    }

    pub fn deregister(vec: u8, cpu: usize) -> sv_call::Result {
        let _pree = PREEMPT.lock();

        if !ALLOC_VEC.contains(&vec) {
            return Err(sv_call::ENOENT);
        }
        let manager = MANAGER.get(cpu).ok_or(sv_call::ENODEV)?;

        *manager.slots[vec as usize].lock() = None;

        {
            let mut map = manager.map.lock();
            manager.count.fetch_sub(1, Ordering::SeqCst);
            map.set(vec as _, false);
        }

        Ok(())
    }

    pub fn allocate_msi(num_vec: u8, cpu: usize) -> sv_call::Result<Msi> {
        const MAX_NUM_VEC: u8 = 32;
        let vec_len = num_vec
            .checked_next_power_of_two()
            .filter(|&size| size <= MAX_NUM_VEC)
            .ok_or(sv_call::EINVAL)?;

        let manager = MANAGER.get(cpu).ok_or(sv_call::ENODEV)?;
        let apic_id = *LAPIC_ID.read().get(&cpu).ok_or(sv_call::EINVAL)?;

        let vec_start = PREEMPT.scope(|| {
            let mut map = manager.map.lock();

            let mut start = ALLOC_VEC_INDEX.start;
            while start + vec_len as usize <= ALLOC_VEC_INDEX.end {
                let slots = map
                    .get_mut(start..(start + vec_len as usize))
                    .ok_or(sv_call::ENOSPC)?;
                match slots.first_one() {
                    Some(delta) => start += delta + 1,
                    None => {
                        slots.fill(true);
                        manager.count.fetch_add(vec_len as usize, Ordering::SeqCst);

                        return Ok(start as u8);
                    }
                }
            }
            Err(sv_call::ENOSPC)
        })?;

        Ok(Msi {
            target_address: minfo::LAPIC_BASE as u32 | (apic_id << 12),
            target_data: vec_start as u32,
            vec_start,
            vec_len,
            apic_id,
        })
    }
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

/// # Safety
///
/// This function must only be called from its assembly routine `rout_XX`.
#[no_mangle]
unsafe extern "C" fn common_interrupt(frame: *mut Frame) {
    let vec = unsafe { &*frame }.errc_vec as u8;
    Manager::invoke(vec);
    super::apic::lapic(|lapic| lapic.eoi());
    crate::sched::SCHED.tick(Instant::now());
}

/// Generic exception handler.
unsafe fn exception(frame_ptr: *mut Frame, vec: def::ExVec) {
    use def::ExVec::*;

    let frame = &mut *frame_ptr;
    if vec == PageFault && crate::mem::space::page_fault(&mut *frame_ptr, frame.errc_vec) {
        return;
    }

    match SCHED.with_current(|cur| Ok(cur.tid().ty())) {
        Ok(task::Type::User) if frame.cs == USR_CODE_X64.into_val().into() => {
            if !task::dispatch_exception(frame, vec) {
                #[cfg(debug_assertions)]
                {
                    let _ = SCHED.with_current(|cur| {
                        log::warn!("Unhandled exception from task {}.", cur.tid().raw());
                        Ok(())
                    });

                    log::error!("{:?}", vec);

                    frame.dump(if vec == PageFault {
                        PageFaultErrCode::FMT
                    } else {
                        Frame::ERRC
                    });
                }
                // Kill the fucking task.
                SCHED.exit_current(sv_call::EFAULT.into_retval(), true)
            }
            // unreachable!()
        }
        _ => {}
    }

    // No more available remedies. Die.
    log::error!("{:?} in the kernel", vec);

    frame.dump(if vec == PageFault {
        PageFaultErrCode::FMT
    } else {
        Frame::ERRC
    });

    archop::halt_loop(Some(false));
}

#[inline]
pub(super) fn init() {
    Azy::force(&MANAGER);
}
