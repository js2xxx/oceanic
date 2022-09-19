pub use crossbeam::epoch::*;

use super::Lazy as SyncLazy;
use crate::thread_local;

static COLLECTOR: SyncLazy<Collector> = SyncLazy::new(Collector::new);

thread_local! {
    static HANDLE: LocalHandle = COLLECTOR.register();
}

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
    HANDLE
        .try_with(|h| f(h))
        .unwrap_or_else(|| f(&COLLECTOR.register()))
}
