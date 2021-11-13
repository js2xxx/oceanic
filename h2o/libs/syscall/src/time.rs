use core::{
    ops::{Add, AddAssign, Sub, SubAssign},
    time::Duration,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(u128);

impl Instant {
    #[cfg(feature = "call")]
    pub fn now() -> Self {
        let mut data = 0;
        crate::call::get_time(&mut data).expect("SYSCALL failed");
        Instant(data)
    }

    #[cfg(feature = "call")]
    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    /// # Safety
    ///
    /// The underlying data can be inconsistent and should not be used with
    /// measurements.
    pub unsafe fn raw(&self) -> u128 {
        self.0
    }

    /// # Safety
    ///
    /// The underlying data can be inconsistent and should not be used with
    /// measurements.
    pub unsafe fn from_raw(raw: u128) -> Self {
        Instant(raw)
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
