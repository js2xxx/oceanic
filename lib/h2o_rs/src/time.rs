use core::{ops::*, time::Duration};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Instant(u128);

impl Instant {
    #[inline]
    pub fn try_now() -> crate::error::Result<Self> {
        let mut data = 0u128;
        sv_call::sv_time_get(&mut data as *mut _ as *mut _).into_res()?;
        // SAFETY: The data represents a valid timestamp.
        Ok(unsafe { Self::from_raw(data) })
    }

    #[inline]
    pub fn now() -> Self {
        Self::try_now().expect("Failed to get current time")
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
    /// The underlying data must represent a valid timestamp.
    #[inline]
    pub const unsafe fn from_raw(data: u128) -> Self {
        Instant(data)
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

#[inline]
pub fn from_us(us: u64) -> Duration {
    if us == u64::MAX {
        Duration::MAX
    } else {
        Duration::from_micros(us)
    }
}