use core::{
    ops::{Add, AddAssign, Sub, SubAssign},
    time::Duration,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    data: u128,
}

impl Instant {
    pub fn now() -> Self {
        let mut data = 0;
        crate::call::get_time(&mut data).expect("SYSCALL failed");
        Instant { data }
    }

    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    /// # Safety
    ///
    /// The underlying data can be inconsistent and should not be used with
    /// measurements.
    pub unsafe fn raw(&self) -> u128 {
        self.data
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Self::Output {
        Instant {
            data: self.data + rhs.as_nanos(),
        }
    }
}

impl AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        self.data += rhs.as_nanos();
    }
}

impl Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Self::Output {
        Instant {
            data: self.data - rhs.as_nanos(),
        }
    }
}

impl SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, rhs: Duration) {
        self.data -= rhs.as_nanos();
    }
}

impl Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Self::Output {
        const NPS: u128 = 1_000_000_000;
        let nanos = self.data - rhs.data;
        Duration::new((nanos / NPS) as u64, (nanos % NPS) as u32)
    }
}
