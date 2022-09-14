use core::{
    cell::Cell,
    fmt,
    marker::PhantomData,
    panic::{RefUnwindSafe, UnwindSafe},
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

use crate::thread::{self, Thread};

type Masked = ();

pub struct Once {
    // `state_and_queue` is actually a pointer to a `Waiter` with extra state
    // bits, so we add the `PhantomData` appropriately.
    state_and_queue: AtomicPtr<Masked>,
    _marker: PhantomData<*const Waiter>,
}

unsafe impl Sync for Once {}
unsafe impl Send for Once {}

impl UnwindSafe for Once {}

impl RefUnwindSafe for Once {}

/// State yielded to [`Once::call_once_force()`]’s closure parameter. The state
/// can be used to query the poison status of the [`Once`].
#[derive(Debug)]
pub struct OnceState {
    poisoned: bool,
    set_state_on_drop_to: Cell<*mut Masked>,
}

// Four states that a Once can be in, encoded into the lower bits of
// `state_and_queue` in the Once structure.
const INCOMPLETE: usize = 0x0;
const POISONED: usize = 0x1;
const RUNNING: usize = 0x2;
const COMPLETE: usize = 0x3;

// Mask to learn about the state. All other bits are the queue of waiters if
// this is in the RUNNING state.
const STATE_MASK: usize = 0x3;

// Representation of a node in the linked list of waiters, used while in the
// RUNNING state.
// Note: `Waiter` can't hold a mutable pointer to the next thread, because then
// `wait` would both hand out a mutable reference to its `Waiter` node, and keep
// a shared reference to check `signaled`. Instead we hold shared references and
// use interior mutability.
#[repr(align(4))] // Ensure the two lower bits are free to use as state bits.
struct Waiter {
    thread: Cell<Option<Thread>>,
    signaled: AtomicBool,
    next: *const Waiter,
}

// Head of a linked list of waiters.
// Every node is a struct on the stack of a waiting thread.
// Will wake up the waiters when it gets dropped, i.e. also on panic.
struct WaiterQueue<'a> {
    state_and_queue: &'a AtomicPtr<Masked>,
    set_state_on_drop_to: *mut Masked,
}

impl Once {
    /// Creates a new `Once` value.
    #[inline]
    #[must_use]
    pub const fn new() -> Once {
        Once {
            state_and_queue: AtomicPtr::new(INCOMPLETE as _),
            _marker: PhantomData,
        }
    }

    #[track_caller]
    pub fn call_once<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        // Fast path check
        if self.is_completed() {
            return;
        }

        let mut f = Some(f);
        self.call_inner(false, &mut |_| f.take().unwrap()());
    }

    pub fn call_once_force<F>(&self, f: F)
    where
        F: FnOnce(&OnceState),
    {
        // Fast path check
        if self.is_completed() {
            return;
        }

        let mut f = Some(f);
        self.call_inner(true, &mut |p| f.take().unwrap()(p));
    }

    #[inline]
    pub fn is_completed(&self) -> bool {
        // An `Acquire` load is enough because that makes all the initialization
        // operations visible to us, and, this being a fast path, weaker
        // ordering helps with performance. This `Acquire` synchronizes with
        // `Release` operations on the slow path.
        self.state_and_queue.load(Ordering::Acquire) as usize == COMPLETE
    }

    // This is a non-generic function to reduce the monomorphization cost of
    // using `call_once` (this isn't exactly a trivial or small implementation).
    //
    // Additionally, this is tagged with `#[cold]` as it should indeed be cold
    // and it helps let LLVM know that calls to this function should be off the
    // fast path. Essentially, this should help generate more straight line code
    // in LLVM.
    //
    // Finally, this takes an `FnMut` instead of a `FnOnce` because there's
    // currently no way to take an `FnOnce` and call it via virtual dispatch
    // without some allocation overhead.
    #[cold]
    #[track_caller]
    fn call_inner(&self, ignore_poisoning: bool, init: &mut dyn FnMut(&OnceState)) {
        let mut state_and_queue = self.state_and_queue.load(Ordering::Acquire);
        loop {
            match state_and_queue as usize {
                COMPLETE => break,
                POISONED if !ignore_poisoning => {
                    // Panic to propagate the poison.
                    panic!("Once instance has previously been poisoned");
                }
                POISONED | INCOMPLETE => {
                    // Try to register this thread as the one RUNNING.
                    let exchange_result = self.state_and_queue.compare_exchange(
                        state_and_queue,
                        RUNNING as _,
                        Ordering::Acquire,
                        Ordering::Acquire,
                    );
                    if let Err(old) = exchange_result {
                        state_and_queue = old;
                        continue;
                    }
                    // `waiter_queue` will manage other waiting threads, and
                    // wake them up on drop.
                    let mut waiter_queue = WaiterQueue {
                        state_and_queue: &self.state_and_queue,
                        set_state_on_drop_to: POISONED as _,
                    };
                    // Run the initialization function, letting it know if we're
                    // poisoned or not.
                    let init_state = OnceState {
                        poisoned: state_and_queue as usize == POISONED,
                        set_state_on_drop_to: Cell::new(COMPLETE as _),
                    };
                    init(&init_state);
                    waiter_queue.set_state_on_drop_to = init_state.set_state_on_drop_to.get();
                    break;
                }
                _ => {
                    // All other values must be RUNNING with possibly a
                    // pointer to the waiter queue in the more significant bits.
                    assert!(state_and_queue as usize & STATE_MASK == RUNNING);
                    wait(&self.state_and_queue, state_and_queue);
                    state_and_queue = self.state_and_queue.load(Ordering::Acquire);
                }
            }
        }
    }
}

fn wait(state_and_queue: &AtomicPtr<Masked>, mut current_state: *mut Masked) {
    // Note: the following code was carefully written to avoid creating a
    // mutable reference to `node` that gets aliased.
    loop {
        // Don't queue this thread if the status is no longer running,
        // otherwise we will not be woken up.
        if current_state as usize & STATE_MASK != RUNNING {
            return;
        }

        // Create the node for our current thread.
        let node = Waiter {
            thread: Cell::new(Some(thread::current())),
            signaled: AtomicBool::new(false),
            next: (current_state as usize & !STATE_MASK) as *const Waiter,
        };
        let me = &node as *const Waiter as *const Masked as *mut Masked;

        // Try to slide in the node at the head of the linked list, making sure
        // that another thread didn't just replace the head of the linked list.
        let exchange_result = state_and_queue.compare_exchange(
            current_state,
            (me as usize | RUNNING) as _,
            Ordering::Release,
            Ordering::Relaxed,
        );
        if let Err(old) = exchange_result {
            current_state = old;
            continue;
        }

        // We have enqueued ourselves, now lets wait.
        // It is important not to return before being signaled, otherwise we
        // would drop our `Waiter` node and leave a hole in the linked list
        // (and a dangling reference). Guard against spurious wakeups by
        // reparking ourselves until we are signaled.
        while !node.signaled.load(Ordering::Acquire) {
            // If the managing thread happens to signal and unpark us before we
            // can park ourselves, the result could be this thread never gets
            // unparked. Luckily `park` comes with the guarantee that if it got
            // an `unpark` just before on an unparked thread it does not park.
            thread::park();
        }
        break;
    }
}

impl fmt::Debug for Once {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Once").finish_non_exhaustive()
    }
}

impl Drop for WaiterQueue<'_> {
    fn drop(&mut self) {
        // Swap out our state with however we finished.
        let state_and_queue = self
            .state_and_queue
            .swap(self.set_state_on_drop_to, Ordering::AcqRel);

        // We should only ever see an old state which was RUNNING.
        assert_eq!(state_and_queue as usize & STATE_MASK, RUNNING);

        // Walk the entire linked list of waiters and wake them up (in lifo
        // order, last to register is first to wake up).
        unsafe {
            // Right after setting `node.signaled = true` the other thread may
            // free `node` if there happens to be has a spurious wakeup.
            // So we have to take out the `thread` field and copy the pointer to
            // `next` first.
            let mut queue = (state_and_queue as usize & !STATE_MASK) as *const Waiter;
            while !queue.is_null() {
                let next = (*queue).next;
                let thread = (*queue).thread.take().unwrap();
                (*queue).signaled.store(true, Ordering::Release);
                // ^- FIXME (maybe): This is another case of issue #55005
                // `store()` has a potentially dangling ref to `signaled`.
                queue = next;
                thread.unpark();
            }
        }
    }
}

impl OnceState {
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub(crate) fn poison(&self) {
        self.set_state_on_drop_to.set(POISONED as _);
    }
}
