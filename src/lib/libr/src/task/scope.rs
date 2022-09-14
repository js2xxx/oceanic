use core::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering::*},
};

use super::*;

pub struct Scope<'scope, 'env: 'scope> {
    data: ScopeData,

    scope: PhantomData<&'scope mut &'scope ()>,
    env: PhantomData<&'env mut &'env ()>,
}

pub struct ScopedJoinHandle<'scope, T>(JoinInner<'scope, T>);

pub(super) struct ScopeData {
    num_running_threads: AtomicUsize,
    main_thread: Thread,
}

impl ScopeData {
    pub(super) fn increment_num_running_threads(&self) {
        if self.num_running_threads.fetch_add(1, Relaxed) > isize::MAX as usize {
            self.decrement_num_running_threads();
            panic!("too many running threads in thread scope");
        }
    }
    pub(super) fn decrement_num_running_threads(&self) {
        if self.num_running_threads.fetch_sub(1, Release) == 1 {
            self.main_thread.unpark();
        }
    }
}

#[track_caller]
pub fn scope<'env, F, T>(f: F) -> T
where
    F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
{
    let scope = Scope {
        data: ScopeData {
            num_running_threads: AtomicUsize::new(0),
            main_thread: current(),
        },
        env: PhantomData,
        scope: PhantomData,
    };

    let result = f(&scope);

    while scope.data.num_running_threads.load(Acquire) != 0 {
        park();
    }
    result
}

impl<'scope, 'env> Scope<'scope, 'env> {
    pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        Builder::new()
            .spawn_scoped(self, f)
            .expect("failed to spawn thread")
    }
}

impl Builder {
    pub fn spawn_scoped<'scope, 'env, F, T>(
        self,
        scope: &'scope Scope<'scope, 'env>,
        f: F,
    ) -> Result<ScopedJoinHandle<'scope, T>>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        Ok(ScopedJoinHandle(unsafe {
            self.spawn_inner(f, Some(&scope.data))
        }?))
    }
}

impl<'scope, T> ScopedJoinHandle<'scope, T> {
    #[must_use]
    pub fn thread(&self) -> &Thread {
        &self.0.thread
    }

    pub fn join(self) -> T {
        self.0.join()
    }

    pub fn is_finished(&self) -> bool {
        Arsc::count(&self.0.packet) == 1
    }
}

impl fmt::Debug for Scope<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope")
            .field(
                "num_running_threads",
                &self.data.num_running_threads.load(Relaxed),
            )
            .field("main_thread", &self.data.main_thread)
            .finish_non_exhaustive()
    }
}

impl<'scope, T> fmt::Debug for ScopedJoinHandle<'scope, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopedJoinHandle").finish_non_exhaustive()
    }
}
