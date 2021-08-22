#[cfg(target_arch = "x86_64")]
use super::arch::tsc::ns_clock;

use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
      #[cfg(target_arch = "x86_64")]
      data: u128,
}

#[cfg(target_arch = "x86_64")]
impl Instant {
      pub fn now() -> Self {
            Instant {
                  data: unsafe { ns_clock() },
            }
      }

      pub fn elapsed(&self) -> Duration {
            Self::now() - *self
      }

      pub unsafe fn raw(&self) -> u128 {
            self.data
      }
}

#[cfg(target_arch = "x86_64")]
impl Add<Duration> for Instant {
      type Output = Instant;

      fn add(self, rhs: Duration) -> Self::Output {
            Instant {
                  data: self.data + rhs.as_nanos(),
            }
      }
}

#[cfg(target_arch = "x86_64")]
impl AddAssign<Duration> for Instant {
      fn add_assign(&mut self, rhs: Duration) {
            self.data += rhs.as_nanos();
      }
}

#[cfg(target_arch = "x86_64")]
impl Sub<Duration> for Instant {
      type Output = Instant;

      fn sub(self, rhs: Duration) -> Self::Output {
            Instant {
                  data: self.data - rhs.as_nanos(),
            }
      }
}

#[cfg(target_arch = "x86_64")]
impl SubAssign<Duration> for Instant {
      fn sub_assign(&mut self, rhs: Duration) {
            self.data -= rhs.as_nanos();
      }
}

#[cfg(target_arch = "x86_64")]
impl Sub<Instant> for Instant {
      type Output = Duration;

      fn sub(self, rhs: Instant) -> Self::Output {
            const NPS: u128 = 1_000_000_000;
            let nanos = self.data - rhs.data;
            Duration::new((nanos / NPS) as u64, (nanos % NPS) as u32)
      }
}

pub fn delay(duration: Duration) {
      let instant = Instant::now();
      while instant.elapsed() < duration {}
}
