use core::{cell::Cell, marker::PhantomData};

use futures::Future;
use solvent_std::thread_local;

thread_local! {
    static STATE: Cell<State> = const { Cell::new(State::NotEntered) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Entered,
    NotEntered,
}

pub(crate) struct EnterGuard {
    _marker: PhantomData<*mut ()>,
}

#[track_caller]
pub(crate) fn try_enter() -> Option<EnterGuard> {
    (STATE.replace(State::Entered) == State::NotEntered).then_some(EnterGuard {
        _marker: PhantomData,
    })
}

#[inline]
#[track_caller]
pub(crate) fn enter() -> EnterGuard {
    try_enter().expect("Cannot start a runtime from within a runtime")
}

impl EnterGuard {
    pub(crate) fn block_on<F: Future>(&mut self, fut: F) -> F::Output {
        super::park::block_on(fut)
    }
}

impl Drop for EnterGuard {
    fn drop(&mut self) {
        STATE.with(|state| {
            assert_eq!(state.get(), State::Entered);
            state.set(State::NotEntered)
        })
    }
}
