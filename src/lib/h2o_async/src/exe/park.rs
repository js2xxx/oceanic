use core::task::{Context, Poll, Waker};

use futures::{pin_mut, Future};
use solvent_std::{sync::Parker, thread_local};
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
    pin_mut!(fut);

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
