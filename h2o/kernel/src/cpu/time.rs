pub mod chip;
mod timer;

use core::{
    ops::{Add, AddAssign, Sub, SubAssign},
    time::Duration,
};

pub use self::timer::{tick as timer_tick, Callback as TimerCallback, Timer, Type as TimerType};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Instant(solvent::time::Instant);

impl Instant {
    #[inline]
    pub fn now() -> Self {
        chip::CLOCK.get()
    }

    #[inline]
    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    /// # Safety
    ///
    /// The underlying data can be inconsistent and should not be used with
    /// measurements.
    #[inline]
    pub const unsafe fn raw(&self) -> u128 {
        self.0.raw()
    }

    /// # Safety
    ///
    /// The underlying data can be inconsistent and should not be used with
    /// measurements.
    #[inline]
    pub const unsafe fn from_raw(data: u128) -> Self {
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
        write!(f, "{:.6}", s)
    }
}

mod syscall {
    use solvent::*;

    use crate::syscall::{Out, UserPtr};

    #[syscall]
    pub(super) fn get_time(ptr: UserPtr<Out, u128>) -> Result {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let raw = super::Instant::now().raw();
            ptr.write(raw)?
        };
        Ok(())
    }
}
