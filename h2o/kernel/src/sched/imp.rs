pub mod deque;
pub mod epoch;
pub mod waiter;

use alloc::vec::Vec;
use core::{
    assert_matches::assert_matches,
    cell::UnsafeCell,
    hint, mem,
    sync::atomic::{AtomicU64, Ordering::*},
    time::Duration,
};

use archop::{Azy, PreemptState, PreemptStateGuard};
use canary::Canary;
use crossbeam_queue::SegQueue;
use deque::{Injector, Steal, Worker};

use super::{ipc::Arsc, task};
use crate::cpu::{
    time::{Instant, Timer},
    Lazy,
};

pub(super) const MIN_TIME_GRAN: Duration = Duration::from_millis(30);
const WAKE_TIME_GRAN: Duration = Duration::from_millis(1);

static SCHED_INFO: Azy<Vec<SchedInfo>> = Azy::new(|| {
    let count = crate::cpu::count();
    core::iter::repeat_with(SchedInfo::default)
        .take(count)
        .collect()
});

#[thread_local]
pub static SCHED: Lazy<Scheduler> = Lazy::new(|| Scheduler {
    canary: Canary::new(),
    cpu: unsafe { crate::cpu::id() },
    current: UnsafeCell::new(None),
    run_queue: Worker::new_fifo(),
});

#[thread_local]
pub static PREEMPT: PreemptState = PreemptState::new();

#[derive(Default)]
struct SchedInfo {
    migration_queue: Injector<task::Ready>,
    expected_runtime: AtomicU64,
}

impl SchedInfo {
    fn expected_runtime(&self) -> u64 {
        self.expected_runtime.load(Acquire)
    }
}

pub struct Scheduler {
    canary: Canary<Scheduler>,
    cpu: usize,
    run_queue: Worker<task::Ready>,
    current: UnsafeCell<Option<task::Ready>>,
}

impl Scheduler {
    pub fn unblock(&self, task: impl task::IntoReady, preempt: bool) {
        self.canary.assert();

        let time_slice = MIN_TIME_GRAN;
        let affinity = task.affinity();
        let cpu = select_cpu(&affinity, self.cpu, task.last_cpu()).expect("Zero affinity");
        let task = task::IntoReady::into_ready(task, cpu, time_slice);

        log::trace!("Unblocking task {:?}, P{}", task.tid.raw(), PREEMPT.raw());
        if cpu == self.cpu {
            self.enqueue(task, PREEMPT.lock(), preempt);
        } else {
            SCHED_INFO[cpu].migration_queue.push(task);
            unsafe { crate::cpu::arch::apic::ipi::task_migrate(cpu) };
        }
    }

    fn enqueue(&self, task: task::Ready, pree: PreemptStateGuard, preempt: bool) {
        SCHED_INFO[self.cpu]
            .expected_runtime
            .fetch_add(task.time_slice.as_millis() as u64, Release);
        // SAFETY: We have `pree`, which means preemption is disabled.
        match unsafe { &*self.current.get() } {
            Some(ref cur) if preempt && Self::should_preempt(cur, &task) => {
                log::trace!(
                    "Preempting to task {:?}, P{}",
                    task.tid.raw(),
                    PREEMPT.raw(),
                );
                let _ = self.schedule_impl(Instant::now(), pree, Some(task), |mut task| {
                    task.running_state = task::RunningState::NOT_RUNNING;
                    self.run_queue.push(task);
                    Ok(())
                });
            }
            _ => self.run_queue.push(task),
        }
    }

    #[inline]
    pub fn with_current<F, R>(&self, func: F) -> sv_call::Result<R>
    where
        F: FnOnce(&mut task::Ready) -> sv_call::Result<R>,
    {
        self.canary.assert();

        // SAFETY: We have `_pree`, which means preemption is disabled.
        PREEMPT.scope(|| unsafe {
            (*self.current.get())
                .as_mut()
                .ok_or(sv_call::ESRCH)
                .and_then(func)
        })
    }

    #[inline]
    pub fn current(&self) -> *mut Option<task::Ready> {
        self.current.get()
    }

    pub fn block_current<T>(
        &self,
        guard: T,
        wq: Option<&SegQueue<Arsc<Timer>>>,
        duration: Duration,
        block_desc: &'static str,
    ) -> sv_call::Result<Arsc<Timer>> {
        self.canary.assert();

        let pree = PREEMPT.lock();

        // SAFETY: We have `pree`, which means preemption is disabled.
        log::trace!(
            "Blocking task {:?}, P{}",
            unsafe { &*self.current.get() }.as_ref().unwrap().tid.raw(),
            PREEMPT.raw(),
        );

        if let Some(current) = unsafe { &*self.current() } {
            SCHED_INFO[self.cpu]
                .expected_runtime
                .fetch_sub(current.time_slice.as_micros() as u64, Release);
        }

        self.schedule_impl(Instant::now(), pree, None, |task| {
            let blocked = task::Ready::block(task, block_desc);
            let timer = Timer::activate(duration, blocked)?;
            if let Some(wq) = wq {
                wq.push(Arsc::clone(&timer));
            }
            drop(guard);
            Ok(timer)
        })
    }

    #[inline]
    fn should_preempt(cur: &task::Ready, task: &task::Ready) -> bool {
        cur.runtime > task.runtime + WAKE_TIME_GRAN
    }

    /// # Panics
    ///
    /// Panics if the scheduler unexpectedly returns.
    pub fn exit_current(&self, retval: usize) -> ! {
        self.canary.assert();
        let pree = PREEMPT.lock();

        // SAFETY: We have `pree`, which means preemption is disabled.
        log::trace!(
            "Exiting task {:?}, P{}",
            unsafe { &*self.current.get() }.as_ref().unwrap().tid.raw(),
            PREEMPT.raw(),
        );

        if let Some(current) = unsafe { &*self.current() } {
            SCHED_INFO[self.cpu]
                .expected_runtime
                .fetch_sub(current.time_slice.as_micros() as u64, Release);
        }

        let _ = self.schedule_impl(Instant::now(), pree, None, |task| {
            task::Ready::exit(task, retval);
            Ok(())
        });
        unreachable!("Dead task");
    }

    pub fn tick(&self, mut cur_time: Instant) {
        log::trace!("Scheduler tick");

        let pree = match self.check_signal(cur_time, PREEMPT.lock()) {
            Some(pree) => pree,
            None => {
                cur_time = Instant::now();
                PREEMPT.lock()
            }
        };

        if unsafe { self.update(cur_time) } {
            let ret = self.schedule(cur_time, pree);
            match ret {
                Ok(()) | Err(sv_call::ENOENT) => {}
                Err(err) => log::warn!("Scheduling failed: {:?}", err),
            }
        }
    }

    fn check_signal<'a>(
        &'a self,
        cur_time: Instant,
        pree: PreemptStateGuard<'a>,
    ) -> Option<PreemptStateGuard<'a>> {
        // SAFETY: We have `pree`, which means preemption is disabled.
        let cur = match unsafe { &*self.current.get() } {
            Some(ref cur) => cur,
            None => return Some(pree),
        };
        log::trace!("Checking task {:?}'s pending signal", cur.tid.raw());
        let ti = &*cur.tid;

        if ti.ty() == task::Type::Kernel {
            return Some(pree);
        }

        match ti.with_signal(|sig| sig.take()) {
            Some(task::Signal::Kill) => {
                log::trace!("Killing task {:?}, P{}", cur.tid.raw(), PREEMPT.raw());

                SCHED_INFO[self.cpu]
                    .expected_runtime
                    .fetch_sub(cur.time_slice.as_micros() as u64, Release);

                let _ = self.schedule_impl(cur_time, pree, None, |task| {
                    task::Ready::exit(task, sv_call::EKILLED.into_retval());
                    Ok(())
                });
                unreachable!("Dead task");
            }
            Some(task::Signal::Suspend(slot)) => {
                log::trace!("Suspending task {:?}, P{}", cur.tid.raw(), PREEMPT.raw());

                SCHED_INFO[self.cpu]
                    .expected_runtime
                    .fetch_sub(cur.time_slice.as_micros() as u64, Release);

                let ret = self.schedule_impl(cur_time, pree, None, |task| {
                    *slot.lock() = Some(task::Ready::block(task, "task_ctl_suspend"));
                    Ok(())
                });
                assert_matches!(ret, Ok(()) | Err(sv_call::ENOENT));

                None
            }
            None => Some(pree),
        }
    }

    unsafe fn update(&self, cur_time: Instant) -> bool {
        self.canary.assert();

        let sole = self.run_queue.is_empty();
        let cur = match *self.current.get() {
            Some(ref mut task) => task,
            None => return !sole,
        };
        log::trace!("Updating task {:?}'s timer slice", cur.tid.raw());

        match cur.running_state.start_time() {
            Some(start_time) => {
                debug_assert!(cur_time > start_time);
                let runtime_delta = cur_time - start_time;
                cur.runtime += runtime_delta;
                if cur.time_slice < runtime_delta && !sole {
                    cur.running_state = task::RunningState::NEED_RESCHED;
                    true
                } else {
                    false
                }
            }
            _ => {
                assert!(cur.running_state.needs_resched(), "Not running");
                true
            }
        }
    }

    fn schedule(&self, cur_time: Instant, pree: PreemptStateGuard) -> sv_call::Result {
        self.canary.assert();
        #[cfg(debug_assertions)]
        // SAFETY: We have `pree`, which means preemption is disabled.
        if let Some(ref cur) = unsafe { &*self.current.get() } {
            log::trace!("Scheduling task {:?}, P{}", cur.tid.raw(), PREEMPT.raw());
        }

        self.schedule_impl(cur_time, pree, None, |mut task| {
            debug_assert!(task.running_state.needs_resched());
            task.running_state = task::RunningState::NOT_RUNNING;
            self.run_queue.push(task);
            Ok(())
        })
    }

    fn schedule_impl<F, R>(
        &self,
        cur_time: Instant,
        pree: PreemptStateGuard,
        next: Option<task::Ready>,
        func: F,
    ) -> sv_call::Result<R>
    where
        F: FnOnce(task::Ready) -> sv_call::Result<R>,
    {
        self.canary.assert();

        let mut next = match next {
            Some(next) => next,
            None => match self.run_queue.pop() {
                Some(task) => task,
                None => return Err(sv_call::ENOENT),
            },
        };
        log::trace!("Switching to task {:?}, P{}", next.tid.raw(), PREEMPT.raw());

        next.running_state = task::RunningState::running(cur_time);
        next.cpu = self.cpu;
        let new = next.kstack.kframe_ptr();

        // SAFETY: We have `pree`, which means preemption is disabled.
        let cur_slot = unsafe { &mut *self.current.get() };
        let (old, ret) = match cur_slot.replace(next) {
            Some(mut prev) => {
                let kframe_mut = prev.kstack.kframe_ptr_mut();
                let ret = func(prev);

                Some((kframe_mut, ret))
            }
            None => None,
        }
        .unzip();

        // We will enable preemption in `switch_ctx`.
        mem::forget(pree);
        unsafe { task::ctx::switch_ctx(old, new) };
        ret.transpose().and_then(|res| res.ok_or(sv_call::ESRCH))
    }
}

fn select_cpu(
    affinity: &crate::cpu::CpuMask,
    cur_cpu: usize,
    last_cpu: Option<usize>,
) -> Option<usize> {
    let mut iter = affinity.iter_ones();
    let mut ret = iter.next()?;

    if ret == cur_cpu && SCHED_INFO[ret].expected_runtime() == 0 {
        return Some(ret);
    }

    for b in iter {
        let rb = SCHED_INFO[b].expected_runtime();
        if b == cur_cpu && rb == 0 {
            return Some(b);
        }

        let a = ret;

        let wlast_cpu = match last_cpu {
            Some(last_cpu) if a == last_cpu && b != last_cpu => 1,
            Some(last_cpu) if a != last_cpu && b == last_cpu => -1,
            _ => 0,
        };

        let wcur_cpu = if a == cur_cpu && b != cur_cpu {
            1
        } else if a != cur_cpu && b == cur_cpu {
            -1
        } else {
            0
        };

        let wruntime = {
            let ra = SCHED_INFO[a].expected_runtime();
            let diff = ra.abs_diff(rb);
            if diff <= 1 {
                0
            } else {
                (diff + 1).ilog2() as i32 * if ra > rb { -1 } else { 1 }
            }
        };

        let weight = wlast_cpu * 10 + wcur_cpu * 2 + wruntime * 20;

        ret = if weight > 0 { a } else { b };
    }

    Some(ret)
}

/// # Safety
///
/// This function must be called only in task-migrate IPI handlers.
pub unsafe fn task_migrate_handler() {
    crate::cpu::arch::apic::lapic(|lapic| lapic.eoi());

    const MAX_TRIAL: usize = 50;
    for _ in 0..MAX_TRIAL {
        match SCHED_INFO[SCHED.cpu].migration_queue.steal() {
            Steal::Empty => break,
            Steal::Retry => hint::spin_loop(),
            Steal::Success(task) => {
                log::trace!("Migrating task {:?}, P{}", task.tid.raw(), PREEMPT.raw());
                let pree = PREEMPT.lock();
                SCHED.enqueue(task, pree, true);
            }
        }
    }
}
