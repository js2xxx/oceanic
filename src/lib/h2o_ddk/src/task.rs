use core::future::Future;

use async_task::Task;
use solvent_async::{disp::DispSender, exe::io_task};
use solvent_core::sync::Lazy;

#[inline]
pub fn spawn<T: Send + 'static>(f: impl Future<Output = T> + Send + 'static) -> Task<T> {
    crate::ffi::global_executor().spawn(f)
}

#[inline]
pub fn spawn_local<T: 'static>(f: impl Future<Output = T> + 'static) -> Task<T> {
    crate::ffi::local_executor(|exe| exe.spawn(f))
}

static DISP: Lazy<DispSender> = Lazy::new(|| {
    let (tx, rx) = solvent_async::disp::dispatch(4096);
    spawn(io_task(rx)).detach();
    tx
});

#[inline]
pub fn dispatch() -> DispSender {
    DISP.clone()
}
