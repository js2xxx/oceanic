pub mod timer;

use core::{
    ops::{Add, AddAssign, Sub, SubAssign},
    time::Duration,
};

pub use timer::{tick as timer_tick, Callback as TimerCallback, Timer};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(solvent::time::Instant);

impl Instant {
    pub fn now() -> Self {
        let mut data = 0;
        let _ = syscall::get_time(&mut data);
        Instant(unsafe { solvent::time::Instant::from_raw(data) })
    }

    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    pub unsafe fn raw(&self) -> u128 {
        self.0.raw()
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

mod syscall {
    use solvent::*;

    #[cfg(target_arch = "x86_64")]
    use crate::cpu::arch::tsc::ns_clock;

    #[syscall]
    pub(super) fn get_time(ptr: *mut u128) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            ptr.write(ns_clock())
        };
        Ok(())
    }
}
