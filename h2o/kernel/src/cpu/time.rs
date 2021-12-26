pub mod chip;
mod timer;

use core::{
    ops::{Add, AddAssign, Sub, SubAssign},
    time::Duration,
};

pub use timer::{tick as timer_tick, Callback as TimerCallback, Timer, Type as TimerType};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(solvent::time::Instant);

impl Instant {
    pub fn now() -> Self {
        let _pree = crate::sched::PREEMPT.lock();
        chip::CLOCK.get()
    }

    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    pub unsafe fn raw(&self) -> u128 {
        self.0.raw()
    }

    pub unsafe fn from_raw(data: u128) -> Self {
        Instant(solvent::time::Instant::from_raw(data))
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Self::Output {
        Instant(self.0 + rhs)
    }
}

impl AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        self.0 += rhs;
    }
}

impl Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Self::Output {
        Instant(self.0 - rhs)
    }
}

impl SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, rhs: Duration) {
        self.0 -= rhs;
    }
}

impl Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Self::Output {
        self.0 - rhs.0
    }
}

pub fn delay(duration: Duration) {
    let instant = Instant::now();
    while instant.elapsed() < duration {}
}

impl core::fmt::Display for Instant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let ns = unsafe { self.raw() };
        let s = ns as f64 / 1_000_000_000.0;
        write!(f, "{:.4} s", s)
    }
}

mod syscall {
    use solvent::*;

    use crate::syscall::{Out, UserPtr};

    #[syscall]
    pub(super) fn get_time(ptr: UserPtr<Out, u128>) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let raw = super::Instant::now().raw();
            ptr.write(raw)?
        };
        Ok(())
    }
}
