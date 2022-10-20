pub(super) mod def;

use array_macro::array;
use collection_ex::RangeMap;
use spin::Mutex;

pub use self::def::{ExVec, ALLOC_VEC};
use super::apic::{Polarity, TriggerMode, LAPIC_ID};
use crate::{
    cpu::{arch::seg::ndt::USR_CODE_X64, intr::IntrHandler, time::Instant, Lazy},
    dev::ioapic,
    sched::{
        task::{self, ctx::arch::Frame},
        PREEMPT, SCHED,
    },
};

#[thread_local]
pub static MANAGER: Lazy<Manager> = Lazy::new(|| Manager::new(unsafe { crate::cpu::id() }));

pub struct Manager {
    cpu: usize,
    map: Mutex<RangeMap<u8, ()>>,
    slots: [Mutex<Option<(IntrHandler, *mut u8)>>; u8::MAX as usize + 1],
}

impl Manager {
    pub fn new(cpu: usize) -> Self {
        Manager {
            cpu,
            map: Mutex::new(RangeMap::new(ALLOC_VEC)),
            slots: array![_ => Mutex::new(None); 256],
        }
    }

    pub fn invoke(&self, vec: u8) {
        PREEMPT.scope(|| {
            if let Some((handler, arg)) = *self.slots[vec as usize].lock() {
                handler(arg);
            } else {
                log::trace!("Unhandled interrupt #{:?}", vec);
            }
        })
    }

    pub fn register(&self, gsi: u32, handler: Option<(IntrHandler, *mut u8)>) -> sv_call::Result {
        let _pree = PREEMPT.lock();
        let mut ioapic = ioapic::chip().lock();
        let entry = ioapic.get_entry(gsi)?;

        let in_use = ALLOC_VEC.contains(&entry.vec());

        let self_apic_id = *LAPIC_ID.read().get(&self.cpu).ok_or(sv_call::EINVAL)?;
        let apic_id = entry.dest_id();
        if in_use && self_apic_id != apic_id {
            return Err(sv_call::EEXIST);
        }

        let vec = in_use.then_some(entry.vec());

        if let Some(handler) = handler {
            let mut map = self.map.lock();
            let vec = if let Some(vec) = vec {
                map.try_insert_with(
                    vec..(vec + 1),
                    || Ok::<_, sv_call::Error>(((), ())),
                    sv_call::EEXIST,
                )?;
                vec
            } else {
                map.allocate_with(1, |_| Ok::<_, sv_call::Error>(((), ())), sv_call::ENOMEM)?
                    .0
            };

            *self.slots[vec as usize].lock() = Some(handler);
            unsafe { ioapic.config_dest(gsi, vec, self_apic_id) }?;
        } else if let Some(vec) = vec {
            *self.slots[vec as usize].lock() = None;
            unsafe { ioapic.deconfig(gsi) }?;
        }
        Ok(())
    }

    #[inline]
    pub fn config(&self, gsi: u32, trig_mode: TriggerMode, polarity: Polarity) -> sv_call::Result {
        PREEMPT.scope(|| unsafe { ioapic::chip().lock().config(gsi, trig_mode, polarity) })
    }

    #[inline]
    pub fn mask(&self, gsi: u32, masked: bool) -> sv_call::Result {
        PREEMPT.scope(|| unsafe { ioapic::chip().lock().mask(gsi, masked) })
    }

    #[inline]
    pub fn eoi(&self, gsi: u32) -> sv_call::Result {
        PREEMPT.scope(|| unsafe { ioapic::chip().lock().eoi(gsi) })
    }
}

/// # Safety
///
/// This function must only be called from its assembly routine `rout_XX`.
#[no_mangle]
unsafe extern "C" fn common_interrupt(frame: *mut Frame) {
    let vec = unsafe { &*frame }.errc_vec as u8;
    MANAGER.invoke(vec);
    super::apic::lapic(|lapic| lapic.eoi());
    crate::sched::SCHED.tick(Instant::now());
}

/// Generic exception handler.
unsafe fn exception(frame_ptr: *mut Frame, vec: def::ExVec) {
    use def::ExVec::*;

    let frame = &mut *frame_ptr;
    match vec {
        PageFault if crate::mem::space::page_fault(&mut *frame_ptr, frame.errc_vec) => return,
        _ => {}
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
                        Frame::ERRC_PF
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
        Frame::ERRC_PF
    } else {
        Frame::ERRC
    });

    archop::halt_loop(Some(false));
}

#[inline]
pub(super) fn init() {
    Lazy::force(&MANAGER);
}
