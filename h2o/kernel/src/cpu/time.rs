pub mod chip;
mod timer;

use core::{
    ops::{Add, AddAssign, Sub, SubAssign},
    time::Duration,
};

pub use self::timer::{tick as timer_tick, Timer};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Instant(u128);

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
        self.0
    }

    /// # Safety
    ///
    /// The underlying data can be inconsistent and should not be used with
    /// measurements.
    #[inline]
    pub const unsafe fn from_raw(data: u128) -> Self {
        Instant(data)
    }

    #[inline]
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        *self - earlier
    }

    #[inline]
    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        (self >= &earlier).then(|| *self - earlier)
    }

    #[inline]
    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.checked_duration_since(earlier)
            .unwrap_or(Duration::ZERO)
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Self::Output {
        Instant(self.0 + rhs.as_nanos())
    }
}

impl AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        self.0 += rhs.as_nanos();
    }
}

impl Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Self::Output {
        Instant(self.0 - rhs.as_nanos())
    }
}

impl SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, rhs: Duration) {
        self.0 -= rhs.as_nanos();
    }
}

impl Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Self::Output {
        const NPS: u128 = 1_000_000_000;
        let nanos = self.0 - rhs.0;
        Duration::new((nanos / NPS) as u64, (nanos % NPS) as u32)
    }
}

pub fn delay(duration: Duration) {
    let instant = Instant::now();
    while instant.elapsed() < duration {}
}

#[inline]
pub fn from_us(us: u64) -> Duration {
    if us == u64::MAX {
        Duration::MAX
    } else {
        Duration::from_micros(us)
    }
}

impl core::fmt::Display for Instant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let ns = unsafe { self.raw() };
        let s = ns as f64 / 1_000_000_000.0;
        write!(f, "{s:.6}")
    }
}

mod syscall {
    use sv_call::*;

    use crate::syscall::{Out, UserPtr};

    #[syscall]
    pub(super) fn time_get(ptr: UserPtr<Out, u128>) -> Result {
        #[cfg(target_arch = "x86_64")]
        ptr.write(unsafe { super::Instant::now().raw() })?;
        Ok(())
    }
}
