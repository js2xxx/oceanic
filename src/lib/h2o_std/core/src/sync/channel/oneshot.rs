use core::{
    cell::UnsafeCell,
    fmt, ptr,
    sync::atomic::{AtomicPtr, Ordering::SeqCst},
};

use solvent::time::Instant;

/// Oneshot channels/ports
///
/// This is the initial flavor of channels/ports used for comm module. This is
/// an optimization for the one-use case of a channel. The major optimization of
/// this type is to have one and exactly one allocation when the chan/port pair
/// is created.
///
/// Another possible optimization would be to not use an Arc box because
/// in theory we know when the shared packet can be deallocated (no real need
/// for the atomic reference counting), but I was having trouble how to destroy
/// the data early in a drop of a Port.
///
/// # Implementation
///
/// Oneshots are implemented around one atomic usize variable. This variable
/// indicates both the state of the port/chan but also contains any threads
/// blocked on the port. All atomic operations happen on this one word.
use self::Failure::*;
use super::{
    blocking::{self, SignalToken},
    RecvError, TryRecvError,
};
use crate::sync::Arsc;

// Various states you can find a port in.
const EMPTY: *mut u8 = ptr::null_mut(); // initial state: no data, no blocked receiver
const DATA: *mut u8 = 1 as _; // data ready for receiver to take
const DISCONNECTED: *mut u8 = 2 as _; // channel is disconnected
                                      // Any other value represents a pointer to a SignalToken value. The
                                      // protocol ensures that when the state moves *to* a pointer,
                                      // ownership of the token is given to the packet, and when the state
                                      // moves *from* a pointer, ownership of the token is transferred to
                                      // whoever changed the state.

pub(crate) struct Packet<T> {
    // Internal state of the chan/port pair (stores the blocked thread as well)
    state: AtomicPtr<u8>,
    // One-shot data slot location
    data: UnsafeCell<Option<T>>,
}

pub enum Failure {
    Empty,
    Disconnected,
}

impl<T> Packet<T> {
    pub fn new() -> Packet<T> {
        Packet {
            data: UnsafeCell::new(None),
            state: AtomicPtr::new(EMPTY),
        }
    }

    pub fn send(&self, t: T) -> Result<(), T> {
        unsafe {
            assert!((*self.data.get()).is_none());
            ptr::write(self.data.get(), Some(t));

            match self.state.swap(DATA, SeqCst) {
                // Sent the data, no one was waiting
                EMPTY => Ok(()),

                // Couldn't send the data, the port hung up first. Return the data
                // back up the stack.
                DISCONNECTED => {
                    self.state.swap(DISCONNECTED, SeqCst);
                    Err((*self.data.get()).take().unwrap())
                }

                // Not possible, these are one-use channels
                DATA => unreachable!(),

                // There is a thread waiting on the other end. We leave the 'DATA'
                // state inside so it'll pick it up on the other end.
                ptr => {
                    SignalToken::from_raw(ptr).signal();
                    Ok(())
                }
            }
        }
    }

    pub fn recv(&self, deadline: Option<Instant>) -> Result<T, Failure> {
        // Attempt to not block the thread (it's a little expensive). If it looks
        // like we're not empty, then immediately go through to `try_recv`.
        if self.state.load(SeqCst) == EMPTY {
            let (wait_token, signal_token) = blocking::tokens();
            let ptr = unsafe { signal_token.into_raw() };

            // race with senders to enter the blocking state
            if self
                .state
                .compare_exchange(EMPTY, ptr, SeqCst, SeqCst)
                .is_ok()
            {
                if let Some(deadline) = deadline {
                    wait_token.wait_max_until(deadline);
                } else {
                    wait_token.wait();
                    debug_assert!(self.state.load(SeqCst) != EMPTY);
                }
            } else {
                // drop the signal token, since we never blocked
                drop(unsafe { SignalToken::from_raw(ptr) });
            }
        }

        self.try_recv()
    }

    pub fn try_recv(&self) -> Result<T, Failure> {
        unsafe {
            match self.state.load(SeqCst) {
                EMPTY => Err(Empty),

                DATA => {
                    let _ = self.state.compare_exchange(DATA, EMPTY, SeqCst, SeqCst);
                    match (*self.data.get()).take() {
                        Some(data) => Ok(data),
                        None => unreachable!(),
                    }
                }

                DISCONNECTED => match (*self.data.get()).take() {
                    Some(data) => Ok(data),
                    None => Err(Disconnected),
                },

                // We are the sole receiver; there cannot be a blocking
                // receiver already.
                _ => unreachable!(),
            }
        }
    }

    pub fn drop_chan(&self) {
        match self.state.swap(DISCONNECTED, SeqCst) {
            DATA | DISCONNECTED | EMPTY => {}

            // If someone's waiting, we gotta wake them up
            ptr => unsafe {
                SignalToken::from_raw(ptr).signal();
            },
        }
    }

    pub fn drop_port(&self) {
        match self.state.swap(DISCONNECTED, SeqCst) {
            // An empty channel has nothing to do, and a remotely disconnected
            // channel also has nothing to do b/c we're about to run the drop
            // glue
            DISCONNECTED | EMPTY => {}

            // There's data on the channel, so make sure we destroy it promptly.
            // This is why not using an arc is a little difficult (need the box
            // to stay valid while we take the data).
            DATA => unsafe {
                (*self.data.get()).take().unwrap();
            },

            // We're the only ones that can block on this port
            _ => unreachable!(),
        }
    }
}

impl<T> Drop for Packet<T> {
    fn drop(&mut self) {
        assert_eq!(self.state.load(SeqCst), DISCONNECTED);
    }
}

pub struct Sender<T> {
    inner: Arsc<Packet<T>>,
}

unsafe impl<T: Send> Send for Sender<T> {}

impl<T> Sender<T> {
    #[inline]
    pub(super) fn new(inner: Arsc<Packet<T>>) -> Self {
        Sender { inner }
    }

    #[inline]
    pub fn send(&self, data: T) -> Result<(), T> {
        self.inner.send(data)
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.inner.drop_chan()
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

pub struct Receiver<T> {
    pub(super) inner: Arsc<Packet<T>>,
}

unsafe impl<T: Send> Send for Receiver<T> {}

impl<T> Receiver<T> {
    #[inline]
    pub(super) fn new(inner: Arsc<Packet<T>>) -> Self {
        Receiver { inner }
    }

    #[inline]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.inner.try_recv().map_err(|failure| match failure {
            Empty => TryRecvError::Empty,
            Disconnected => TryRecvError::Disconnected,
        })
    }

    #[inline]
    pub fn recv(&self) -> Result<T, RecvError> {
        self.inner.recv(None).map_err(|_| RecvError)
    }
}

impl<T> Drop for Receiver<T> {
    #[inline]
    fn drop(&mut self) {
        self.inner.drop_port()
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}
