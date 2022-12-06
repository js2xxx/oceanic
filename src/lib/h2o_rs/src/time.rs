use core::{ops::*, time::Duration};

use sv_call::{ETIME, SV_TIMER};

use crate::{
    error::{Error, Result},
    obj::Object,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Instant(u128);

impl Instant {
    #[inline]
    pub fn try_now() -> Result<Self> {
        let mut data = 0u128;
        unsafe { sv_call::sv_time_get(&mut data as *mut _ as *mut _).into_res()? };
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

pub fn try_into_us(duration: Duration) -> Result<u64> {
    if duration == Duration::MAX {
        Ok(u64::MAX)
    } else {
        u64::try_from(duration.as_micros()).map_err(Error::from)
    }
}

impl core::fmt::Display for Instant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let ns = unsafe { self.raw() };
        let s = ns as f64 / 1_000_000_000.0;
        write!(f, "{s:.6}")
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct Timer(sv_call::Handle);
crate::impl_obj!(Timer, SV_TIMER);
crate::impl_obj!(@CLONE, Timer);
crate::impl_obj!(@DROP, Timer);

impl Timer {
    pub fn try_new() -> Result<Self> {
        let handle = unsafe { sv_call::sv_timer_new() }.into_res()?;
        // SAFETY: The handle is freshly allocated.
        Ok(unsafe { Self::from_raw(handle) })
    }

    pub fn new() -> Self {
        Self::try_new().expect("Failed to create a timer object")
    }

    /// If the timer is already set, then it will be canceled first (sending the
    /// cancellation event).
    ///
    /// If `duration` is zero ([`Duration::ZERO`]), then the timer will not be
    /// triggered.
    pub fn set(&self, duration: Duration) -> Result {
        // SAFETY: We don't move the ownership of the handle.
        unsafe { sv_call::sv_timer_set(unsafe { self.raw() }, try_into_us(duration)?) }.into_res()
    }

    /// See [`Timer::set`] for more information.
    #[inline]
    pub fn set_deadline(&self, deadline: Instant) -> Result {
        let now = Instant::now();
        if deadline <= now {
            Err(ETIME)
        } else {
            self.set(deadline - now)
        }
    }

    /// Shorthand for `set(Duration::ZERO)`.
    pub fn reset(&self) -> Result {
        self.set(Duration::ZERO)
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
