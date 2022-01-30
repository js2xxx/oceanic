use core::time::Duration;

use spin::Mutex;

use super::arch::MANAGER;
use crate::{
    cpu::time::Instant,
    dev::Resource,
    sched::{ipc::Event, PREEMPT},
};

pub struct Interrupt {
    gsi: u32,
    event: Event,
    last_time: Mutex<Option<Instant>>,
    level_triggered: bool,
}

impl Interrupt {
    #[inline]
    pub fn new(res: &Resource<u32>, gsi: u32, level_triggered: bool) -> sv_call::Result<Self> {
        if res.magic_eq(super::gsi_resource()) && res.range().contains(&gsi) {
            Ok(Interrupt {
                gsi,
                event: Event::new(false),
                last_time: Mutex::new(None),
                level_triggered,
            })
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

    pub fn handle(&self) {
        PREEMPT.scope(|| *self.last_time.lock() = Some(Instant::now()));
        let _ = self.event.notify(u8::MAX);
        if self.level_triggered {
            MANAGER.mask(self.gsi, true).unwrap();
        }
    }

    pub fn wait(&self, timeout: Duration, block_desc: &'static str) -> (Instant, sv_call::Result) {
        if self.level_triggered {
            MANAGER.mask(self.gsi, false).unwrap();
        }
        let ret = self.event.wait(u8::MAX, timeout, block_desc);
        let t = self.last_time().unwrap();
        (t, ret)
    }
}

fn handler(arg: *mut u8) {
    let intr = unsafe { &*arg.cast::<Interrupt>() };
    intr.handle()
}

mod syscall {
    use alloc::{boxed::Box, sync::Arc};
    use core::time::Duration;

    use sv_call::*;

    use super::*;
    use crate::{
        cpu::{
            arch::apic::{Polarity, TriggerMode},
            intr::arch::MANAGER,
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
            let intr = Interrupt::new(res, gsi, level_triggered)?;
            Box::try_new(intr).map_err(Error::from)
        })?;

        MANAGER.config(gsi, trig_mode, polarity)?;
        MANAGER.register(
            gsi,
            Some((handler, (&*intr as *const Interrupt) as *mut u8)),
        )?;
        MANAGER.mask(gsi, false)?;

        SCHED.with_current(|cur| cur.space().handles().insert(intr))
    }

    #[syscall]
    fn intr_wait(hdl: Handle, timeout_us: u64, last_time: UserPtr<Out, u128>) -> Result {
        hdl.check_null()?;
        last_time.check()?;
        let timeout = if timeout_us == u64::MAX {
            Duration::MAX
        } else {
            Duration::from_micros(timeout_us)
        };
        SCHED.with_current(|cur| {
            let intr = cur.space().handles().get::<Box<Interrupt>>(hdl)?;
            let (t, ret) = intr.wait(timeout, "intr_wait");
            unsafe { last_time.write(t.raw()) }?;
            ret
        })
    }

    #[syscall]
    fn intr_drop(hdl: Handle) -> Result {
        hdl.check_null()?;
        SCHED.with_current(|cur| {
            let intr = cur.space().handles().remove::<Box<Interrupt>>(hdl)?;
            let intr = intr.downcast_ref::<Box<Interrupt>>()?;
            MANAGER.register(intr.gsi, None)?;
            Ok(())
        })
    }
}
