use core::{
    future::Future,
    task::{Context, Poll, Waker},
};

use futures_lite::pin;
use solvent_core::{thread, thread_local};
use waker_fn::waker_fn;

thread_local! {
    static CURRENT: Waker = {
        let thread = thread::current();
        waker_fn(move || thread.unpark())
    }
}

pub(super) fn block_on<F: Future>(fut: F) -> F::Output {
    pin!(fut);

    CURRENT.with(|waker| {
        let mut cx = Context::from_waker(waker);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(v) => break v,
                Poll::Pending => thread::park(),
            }
        }
    })
}
