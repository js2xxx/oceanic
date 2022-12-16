use core::{
    future::Future,
    task::{Context, Poll, Waker},
};

use futures_lite::pin;
use solvent_core::{sync::Parker, thread_local};
use waker_fn::waker_fn;

thread_local! {
    static CURRENT: (Parker, Waker) = {
        let parker = Parker::new();
        let unparker = parker.unparker().clone();
        let waker = waker_fn(move || unparker.unpark());
        (parker, waker)
    }
}

pub(crate) fn block_on<F: Future>(fut: F) -> F::Output {
    pin!(fut);

    CURRENT.with(|(parker, waker)| {
        let mut cx = Context::from_waker(waker);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                break v;
            }

            parker.park();
        }
    })
}
