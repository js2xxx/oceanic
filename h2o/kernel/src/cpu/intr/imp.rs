use alloc::sync::Arc;

use spin::Mutex;

use super::arch::MANAGER;
use crate::{
    cpu::time::Instant,
    dev::Resource,
    sched::{Event, EventData, PREEMPT, SIG_GENERIC},
};

#[derive(Debug)]
pub struct Interrupt {
    gsi: u32,
    last_time: Mutex<Option<Instant>>,
    level_triggered: bool,
    event_data: EventData,
}

impl Event for Interrupt {
    fn event_data(&self) -> &EventData {
        &self.event_data
    }

    fn wait(&self, waiter: Arc<dyn crate::sched::Waiter>) {
        if self.level_triggered {
            MANAGER.mask(self.gsi, false).unwrap();
        }
        self.wait_impl(waiter);
    }

    fn notify(&self, clear: usize, set: usize) {
        PREEMPT.scope(|| *self.last_time.lock() = Some(Instant::now()));
        self.notify_impl(clear, set);
        if self.level_triggered {
            MANAGER.mask(self.gsi, true).unwrap();
        }
    }
}

impl Interrupt {
    #[inline]
    pub fn new(res: &Resource<u32>, gsi: u32, level_triggered: bool) -> sv_call::Result<Arc<Self>> {
        if res.magic_eq(super::gsi_resource()) && res.range().contains(&gsi) {
            Ok(Arc::new(Interrupt {
                gsi,
                last_time: Mutex::new(None),
                level_triggered,
                event_data: EventData::new(0),
            }))
        } else {
            Err(sv_call::Error::EPERM)
        }
    }

    #[inline]
    pub fn last_time(&self) -> Option<Instant> {
        PREEMPT.scope(|| *self.last_time.lock())
    }

    #[inline]
    pub fn gsi(&self) -> u32 {
        self.gsi
    }
}

fn handler(arg: *mut u8) {
    let intr = unsafe { &*arg.cast::<Interrupt>() };
    intr.notify(0, SIG_GENERIC);
}

mod syscall {
    use alloc::sync::Arc;

    use sv_call::*;

    use super::*;
    use crate::{
        cpu::{
            arch::apic::{Polarity, TriggerMode},
            intr::arch::MANAGER,
            time,
        },
        sched::SCHED,
        syscall::{Out, UserPtr},
    };

    bitflags::bitflags! {
        struct IntrConfig: u32 {
            const ACTIVE_HIGH     = 0b01;
            const LEVEL_TRIGGERED = 0b10;
        }
    }

    #[syscall]
    fn intr_new(res: Handle, gsi: u32, config: u32) -> Result<Handle> {
        let config = IntrConfig::from_bits(config).ok_or(Error::EINVAL)?;
        let level_triggered = config.contains(IntrConfig::LEVEL_TRIGGERED);
        let trig_mode = if level_triggered {
            TriggerMode::Level
        } else {
            TriggerMode::Edge
        };
        let polarity = if config.contains(IntrConfig::ACTIVE_HIGH) {
            Polarity::High
        } else {
            Polarity::Low
        };

        let intr = SCHED.with_current(|cur| {
            let handles = cur.space().handles();
            let res = handles.get::<Arc<Resource<u32>>>(res)?;
            Interrupt::new(res, gsi, level_triggered)
        })?;

        MANAGER.config(gsi, trig_mode, polarity)?;
        MANAGER.register(
            gsi,
            Some((handler, (&*intr as *const Interrupt) as *mut u8)),
        )?;
        MANAGER.mask(gsi, false)?;

        let event = Arc::downgrade(&intr) as _;
        SCHED.with_current(|cur| unsafe { cur.space().handles().insert_event(intr, event) })
    }

    #[syscall]
    fn intr_wait(hdl: Handle, timeout_us: u64, last_time: UserPtr<Out, u128>) -> Result {
        hdl.check_null()?;
        last_time.check()?;

        let pree = PREEMPT.lock();
        let intr = unsafe { (*SCHED.current()).as_ref().ok_or(Error::ESRCH)? }
            .space()
            .handles()
            .get::<Arc<Interrupt>>(hdl)?;

        let blocker = crate::sched::Blocker::new(&(Arc::clone(intr) as _), false, SIG_GENERIC);
        blocker.wait(pree, time::from_us(timeout_us))?;
        if !blocker.detach().0 {
            return Err(Error::ETIME);
        }

        unsafe { last_time.write(intr.last_time().unwrap().raw()) }?;
        Ok(())
    }

    #[syscall]
    fn intr_drop(hdl: Handle) -> Result {
        hdl.check_null()?;
        SCHED.with_current(|cur| {
            let intr = cur.space().handles().remove::<Arc<Interrupt>>(hdl)?;
            let intr = intr.downcast_ref::<Arc<Interrupt>>()?;
            intr.cancel();
            MANAGER.register(intr.gsi, None)?;
            Ok(())
        })
    }
}
