//! This module started its life as crossbeam-epoch.

pub use crossbeam_epoch::*;

use spin::Lazy;

/// The global data for the default garbage collector.
static COLLECTOR: Lazy<Collector> = Lazy::new(Collector::new);

/// The per-thread participant for the default garbage collector.
#[thread_local]
static HANDLE: Lazy<LocalHandle> = Lazy::new(|| COLLECTOR.register());

/// Pins the current thread.
#[inline]
pub fn pin() -> Guard {
      with_handle(|handle| handle.pin())
}

/// Returns `true` if the current thread is pinned.
#[inline]
pub fn is_pinned() -> bool {
      with_handle(|handle| handle.is_pinned())
}

/// Returns the default global collector.
pub fn default_collector() -> &'static Collector {
      &COLLECTOR
}

#[inline]
fn with_handle<F, R>(mut f: F) -> R
where
      F: FnMut(&LocalHandle) -> R,
{
      f(&HANDLE)
}
