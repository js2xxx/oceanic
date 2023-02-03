use alloc::sync::Arc;

use crossbeam_queue::ArrayQueue;
use sv_call::Feature;

use super::arch::MANAGER;
use crate::{
    cpu::time::Instant,
    dev::Resource,
    sched::{task::hdl::DefaultFeature, Event, EventData, SIG_GENERIC},
};

const MAX_TIMES: usize = 100;

#[derive(Debug)]
pub struct Interrupt {
    gsi: u32,
    last_time: ArrayQueue<Instant>,
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

    fn notify(&self, clear: usize, set: usize) -> usize {
        self.last_time.force_push(Instant::now());

        let signal = self.notify_impl(clear, set);

        if self.level_triggered {
            MANAGER.mask(self.gsi, true).unwrap();
        }
        MANAGER.eoi(self.gsi).unwrap();
        signal
    }
}

impl Interrupt {
    #[inline]
    pub fn new(res: &Resource<u32>, gsi: u32, level_triggered: bool) -> sv_call::Result<Arc<Self>> {
        if res.magic_eq(super::gsi_resource()) && res.range().contains(&gsi) {
            Ok(Arc::try_new(Interrupt {
                gsi,
                last_time: ArrayQueue::new(MAX_TIMES),
                level_triggered,
                event_data: EventData::new(0),
            })?)
        } else {
            Err(sv_call::EPERM)
        }
    }

    #[inline]
    pub fn last_time(&self) -> Option<Instant> {
        self.last_time.pop()
    }

    #[inline]
    pub fn gsi(&self) -> u32 {
        self.gsi
    }
}

unsafe impl DefaultFeature for Interrupt {
    fn default_features() -> Feature {
        Feature::SEND | Feature::WAIT
    }
}

fn handler(arg: *mut u8) {
    let intr = unsafe { &*arg.cast::<Interrupt>() };
    intr.notify(0, SIG_GENERIC);
}

mod syscall {
    use alloc::sync::Arc;

    use sv_call::{res::IntrConfig, *};

    use super::*;
    use crate::{
        cpu::{
            arch::apic::{Polarity, TriggerMode},
            intr::arch::MANAGER,
        },
        sched::SCHED,
        syscall::{Out, UserPtr},
    };

    #[syscall]
    fn intr_new(res: Handle, gsi: u32, config: IntrConfig) -> Result<Handle> {
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
            let res = handles.get::<Resource<u32>>(res)?;
            Interrupt::new(&res, gsi, level_triggered)
        })?;

        MANAGER.config(gsi, trig_mode, polarity)?;
        MANAGER.register(
            gsi,
            Some((handler, (&*intr as *const Interrupt) as *mut u8)),
        )?;
        MANAGER.mask(gsi, false)?;

        let event = Arc::downgrade(&intr) as _;
        SCHED.with_current(|cur| unsafe { cur.space().handles().insert_raw(intr, Some(event)) })
    }

    #[syscall]
    fn intr_query(hdl: Handle, last_time: UserPtr<Out, u128>) -> Result {
        hdl.check_null()?;
        last_time.check()?;

        SCHED.with_current(|cur| {
            let intr = cur.space().handles().get::<Interrupt>(hdl)?;
            let data = intr.last_time().ok_or(ENOENT)?;
            last_time.write(unsafe { data.raw() })
        })
    }

    #[syscall]
    fn intr_drop(hdl: Handle) -> Result {
        hdl.check_null()?;
        SCHED.with_current(|cur| {
            let intr = cur.space().handles().remove::<Interrupt>(hdl)?;
            intr.cancel();
            MANAGER.register(intr.gsi, None)?;
            Ok(())
        })
    }
}
