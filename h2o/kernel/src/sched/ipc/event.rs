use core::{
    sync::atomic::{AtomicUsize, Ordering::*},
    time::Duration,
};

use sv_call::SerdeReg;

use crate::sched::wait::WaitObject;

pub struct Event {
    wo: WaitObject,
    wake_all: bool,
    signal: AtomicUsize,
}

impl Event {
    #[inline]
    pub fn new(wake_all: bool) -> Self {
        Event {
            wo: WaitObject::new(),
            wake_all,
            signal: AtomicUsize::default(),
        }
    }

    pub fn wait(&self, signal: u8, timeout: Duration, block_desc: &'static str) -> sv_call::Result {
        let signal = signal as usize;
        loop {
            let ret = self.signal.load(SeqCst);
            if ret & signal == signal {
                if !self.wake_all {
                    self.signal.store(ret & !signal, SeqCst);
                }
                break sv_call::Result::decode(ret);
            }
            if !self.wo.wait((), timeout, block_desc) {
                break Err(sv_call::Error::EPIPE);
            }
        }
    }

    #[inline]
    pub fn notify(&self, active: u8) -> sv_call::Result<usize> {
        let active = active as usize;
        if active == 0 {
            Err(sv_call::Error::EINVAL)
        } else {
            let _ = self
                .signal
                .fetch_update(SeqCst, SeqCst, |s| Some(s | active));
            let ret = self.wo.notify(if self.wake_all { usize::MAX } else { 1 });
            Ok(ret)
        }
    }

    #[inline]
    pub fn end_notify(&self, masked: u8) {
        let _ = self
            .signal
            .fetch_update(SeqCst, SeqCst, |s| Some(s & !masked as usize));
    }
}

mod syscall {
    use sv_call::*;

    use super::*;
    use crate::sched::SCHED;

    #[syscall]
    fn event_new(wake_all: bool) -> Result<Handle> {
        let event = Event::new(wake_all);
        SCHED.with_current(|cur| cur.space().handles().insert(event))
    }

    #[syscall]
    fn event_wait(hdl: Handle, signal: u8, timeout_us: u64) -> Result {
        hdl.check_null()?;
        let timeout = if timeout_us == u64::MAX {
            Duration::MAX
        } else {
            Duration::from_micros(timeout_us)
        };
        SCHED.with_current(|cur| {
            cur.space()
                .handles()
                .get::<Event>(hdl)
                .and_then(|event| event.wait(signal, timeout, "event_wait"))
        })
    }

    #[syscall]
    fn event_notify(hdl: Handle, active: u8) -> Result<usize> {
        hdl.check_null()?;
        SCHED.with_current(|cur| {
            cur.space()
                .handles()
                .get::<Event>(hdl)
                .and_then(|event| event.notify(active))
        })
    }

    #[syscall]
    fn event_endn(hdl: Handle, masked: u8) -> Result {
        hdl.check_null()?;
        SCHED.with_current(|cur| {
            cur.space()
                .handles()
                .get::<Event>(hdl)
                .map(|event| event.end_notify(masked))
        })
    }
}
