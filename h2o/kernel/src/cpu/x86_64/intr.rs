pub(super) mod def;

use alloc::vec::Vec;
use core::{
    iter,
    sync::atomic::{AtomicUsize, Ordering},
};

use archop::Azy;
use array_macro::array;
use collection_ex::RangeMap;
use spin::Mutex;

pub use self::def::{ExVec, ALLOC_VEC};
use super::apic::{Polarity, TriggerMode, LAPIC_ID};
use crate::{
    cpu::{
        arch::seg::ndt::USR_CODE_X64,
        intr::{IntrHandler, Msi},
        time::Instant,
    },
    dev::ioapic,
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

pub struct Manager {
    map: Mutex<RangeMap<u8, ()>>,
    slots: [Mutex<Option<(IntrHandler, *mut u8)>>; u8::MAX as usize + 1],
    count: AtomicUsize,
}

unsafe impl Sync for Manager {}
unsafe impl Send for Manager {}

impl Manager {
    pub fn new() -> Self {
        Manager {
            map: Mutex::new(RangeMap::new(ALLOC_VEC)),
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

    #[inline]
    pub fn config(gsi: u32, trig_mode: TriggerMode, polarity: Polarity) -> sv_call::Result {
        PREEMPT.scope(|| unsafe { ioapic::chip().lock().config(gsi, trig_mode, polarity) })
    }

    #[inline]
    pub fn mask(gsi: u32, masked: bool) -> sv_call::Result {
        PREEMPT.scope(|| unsafe { ioapic::chip().lock().mask(gsi, masked) })
    }

    #[inline]
    pub fn eoi(gsi: u32) -> sv_call::Result {
        PREEMPT.scope(|| unsafe { ioapic::chip().lock().eoi(gsi) })
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

    pub fn register(gsi: u32, cpu: usize, handler: (IntrHandler, *mut u8)) -> sv_call::Result {
        let _pree = PREEMPT.lock();
        let mut ioapic = ioapic::chip().lock();
        let entry = ioapic.get_entry(gsi)?;

        if ALLOC_VEC.contains(&entry.vec()) {
            return Err(sv_call::EEXIST);
        }

        let apic_id = *LAPIC_ID.read().get(&cpu).ok_or(sv_call::EINVAL)?;
        let manager = MANAGER.get(cpu).ok_or(sv_call::ENODEV)?;

        let vec = manager.map.lock().allocate_with(
            1,
            |_| {
                manager.count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            sv_call::ENOMEM,
        )?;

        *manager.slots[vec as usize].lock() = Some(handler);
        unsafe { ioapic.config_dest(gsi, vec, apic_id) }?;

        Ok(())
    }

    pub fn deregister(gsi: u32, cpu: usize) -> sv_call::Result {
        let _pree = PREEMPT.lock();
        let mut ioapic = ioapic::chip().lock();
        let entry = ioapic.get_entry(gsi)?;

        let vec = entry.vec();

        if !ALLOC_VEC.contains(&vec) {
            return Err(sv_call::ENOENT);
        }
        let manager = MANAGER.get(cpu).ok_or(sv_call::ENODEV)?;

        *manager.slots[vec as usize].lock() = None;
        unsafe { ioapic.deconfig(gsi) }?;

        {
            let mut lock = manager.map.lock();
            manager.count.fetch_sub(1, Ordering::SeqCst);
            lock.remove(vec);
        }

        Ok(())
    }

    pub fn allocate_msi(num_vec: u8, cpu: usize) -> sv_call::Result<Msi> {
        const MAX_NUM_VEC: u8 = 32;
        let num_vec = num_vec
            .checked_next_power_of_two()
            .filter(|&size| size <= MAX_NUM_VEC)
            .ok_or(sv_call::EINVAL)?;

        let manager = MANAGER.get(cpu).ok_or(sv_call::ENODEV)?;
        let apic_id = *LAPIC_ID.read().get(&cpu).ok_or(sv_call::EINVAL)?;

        let start = PREEMPT.scope(|| {
            manager.map.lock().allocate_with(
                num_vec,
                |_| {
                    manager.count.fetch_add(num_vec as usize, Ordering::SeqCst);
                    Ok(())
                },
                sv_call::ENOMEM,
            )
        })?;

        Ok(Msi {
            target_address: minfo::LAPIC_BASE as u32 | (apic_id << 12),
            target_data: start as u32,
            vecs: start..(start + num_vec),
            cpu,
        })
    }

    pub fn deallocate_msi(msi: Msi) -> sv_call::Result {
        let manager = MANAGER.get(msi.cpu).ok_or(sv_call::ENODEV)?;
        PREEMPT.scope(|| manager.map.lock().remove(msi.vecs.start));
        Ok(())
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
