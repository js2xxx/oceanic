pub mod deque;
pub mod epoch;

use alloc::vec::Vec;
use core::time::Duration;

use archop::{PreemptMutex, PreemptMutexGuard};
use canary::Canary;
use deque::{Injector, Steal, Worker};
use spin::Lazy;

use super::task;
use crate::cpu::time::Instant;

const MINIMUM_TIME_GRANULARITY: Duration = Duration::from_millis(30);
const WAKE_TIME_GRANULARITY: Duration = Duration::from_millis(1);

pub static MIGRATION_QUEUE: Lazy<Vec<Injector<task::Ready>>> = Lazy::new(|| {
    let count = crate::cpu::count();
    core::iter::repeat_with(Injector::new).take(count).collect()
});

#[thread_local]
pub static SCHED: Lazy<Scheduler> = Lazy::new(|| Scheduler {
    canary: Canary::new(),
    cpu: unsafe { crate::cpu::id() },
    current: Current::new(None),
    run_queue: Worker::new_fifo(),
});

type Current = PreemptMutex<Option<task::Ready>>;
type CurrentGuard<'a> = PreemptMutexGuard<'a, Option<task::Ready>>;

pub struct Scheduler {
    canary: Canary<Scheduler>,
    cpu: usize,
    run_queue: Worker<task::Ready>,
    pub(in crate::sched) current: Current,
}

impl Scheduler {
    pub fn push(&self, task: task::Init) {
        self.canary.assert();

        let affinity = task.tid().info().read().affinity();

        let time_slice = MINIMUM_TIME_GRANULARITY;
        if !affinity.get(self.cpu).map_or(false, |r| *r) {
            let cpu = select_cpu(&affinity).expect("Zero affinity");
            let task = task::Ready::from_init(task, cpu, time_slice);
            MIGRATION_QUEUE[cpu].push(task);

            unsafe { crate::cpu::arch::apic::ipi::task_migrate(cpu) };
        } else {
            let task = task::Ready::from_init(task, self.cpu, time_slice);

            let cur = self.current.lock();
            self.enqueue(task, cur);
        }
    }

    fn enqueue(&self, task: task::Ready, cur: CurrentGuard) {
        if Self::should_preempt(&cur, &task) {
            self.schedule_impl(Instant::now(), cur, Some(task), |mut task| {
                task.running_state = task::RunningState::NotRunning;
                self.run_queue.push(task);
            });
        } else {
            drop(cur);
            self.run_queue.push(task);
        }
    }

    pub fn with_current<F, R>(&self, func: F) -> Option<R>
    where
        F: FnOnce(&mut task::Ready) -> R,
    {
        self.canary.assert();

        let mut cur = self.current.lock();
        cur.as_mut().map(func)
    }

    pub unsafe fn preempt_current(&self) {
        self.canary.assert();

        if let Some(ref mut cur) = &mut *self.current.lock() {
            cur.running_state = task::RunningState::NeedResched;
        }
    }

    pub fn block_current<T>(
        &self,
        cur_time: Instant,
        guard: T,
        wo: &super::wait::WaitObject,
        block_desc: &'static str,
    ) -> bool {
        self.canary.assert();
        let cur = self.current.lock();

        self.schedule_impl(cur_time, cur, None, |task| {
            task::Ready::block(task, wo, block_desc);
            drop(guard);
        })
        .is_some()
    }

    pub fn unblock(&self, task: task::Blocked) {
        self.canary.assert();

        let time_slice = MINIMUM_TIME_GRANULARITY;
        let task = task::Ready::unblock(task, time_slice);
        if task.cpu == self.cpu {
            let cur = self.current.lock();
            self.enqueue(task, cur);
        } else {
            let cpu = task.cpu;
            MIGRATION_QUEUE[cpu].push(task);
            unsafe { crate::cpu::arch::apic::ipi::task_migrate(cpu) };
        }
    }

    fn should_preempt(cur: &CurrentGuard, task: &task::Ready) -> bool {
        match cur.as_ref() {
            Some(cur) if cur.runtime > task.runtime + WAKE_TIME_GRANULARITY => true,
            _ => false,
        }
    }

    pub fn exit_current(&self, retval: usize) -> ! {
        self.canary.assert();
        let cur = self.current.lock();

        self.schedule_impl(Instant::now(), cur, None, |task| {
            task::Ready::exit(task, retval)
        });
        unreachable!("Dead task");
    }

    pub fn tick(&self, cur_time: Instant) {
        let cur = self.current.lock();
        let mut cur = self.check_signal(cur_time, cur);
        let need_resched = self.update(cur_time, &mut cur);

        if need_resched {
            self.schedule(cur_time, cur);
        }
    }

    fn check_signal<'a>(
        &'a self,
        cur_time: Instant,
        cur_guard: CurrentGuard<'a>,
    ) -> CurrentGuard<'a> {
        let cur = match &*cur_guard {
            Some(cur) => cur,
            None => return cur_guard,
        };
        let ti = cur.tid().info().read();
        match ti.signal() {
            Some(task::sig::Signal::Kill) => {
                drop(ti);
                self.schedule_impl(cur_time, cur_guard, None, |task| {
                    task::Ready::exit(task, (-solvent::EKILLED) as usize)
                });
                unreachable!("Dead task");
            }
            Some(task::sig::Signal::Suspend) => {
                // ti.set_signal(None);
                drop(ti);
                // TODO: Suspend the current task.
                // self.schedule_impl(cur_time, cur_guard, None, |task|
                // todo!())
                cur_guard
            }
            None => {
                drop(ti);
                cur_guard
            }
        }
    }

    fn update(&self, cur_time: Instant, cur: &mut CurrentGuard) -> bool {
        self.canary.assert();

        let sole = self.run_queue.is_empty();
        let cur = match cur.as_mut() {
            Some(task) => task,
            None => return !sole,
        };

        match &cur.running_state {
            task::RunningState::Running(start_time) => {
                debug_assert!(cur_time > *start_time);
                let runtime_delta = cur_time - *start_time;
                cur.runtime += runtime_delta;
                if cur.time_slice() < runtime_delta && !sole {
                    cur.running_state = task::RunningState::NeedResched;
                    true
                } else {
                    false
                }
            }
            task::RunningState::NotRunning => panic!("Not running"),
            task::RunningState::NeedResched => true,
        }
    }

    fn schedule(&self, cur_time: Instant, cur: CurrentGuard) -> bool {
        self.canary.assert();

        self.schedule_impl(cur_time, cur, None, |mut task| {
            debug_assert!(matches!(
                task.running_state,
                task::RunningState::NeedResched
            ));
            task.running_state = task::RunningState::NotRunning;
            self.run_queue.push(task);
        })
        .is_some()
    }

    fn schedule_impl<F, R>(
        &self,
        cur_time: Instant,
        mut cur: CurrentGuard,
        next: Option<task::Ready>,
        func: F,
    ) -> Option<R>
    where
        F: FnOnce(task::Ready) -> R,
    {
        self.canary.assert();

        let cur_cpu = self.cpu;
        let mut next = match next {
            Some(next) => next,
            None => match self.run_queue.pop() {
                Some(task) => task,
                None => return None,
            },
        };

        next.running_state = task::RunningState::Running(cur_time);
        next.cpu = cur_cpu;
        let new_kframe = next.kframe();

        let (kframe, ret) = match cur.replace(next) {
            Some(mut prev) => {
                let prev_kframe = prev.kframe_mut();
                let ret = func(prev);

                Some((prev_kframe, ret))
            }
            None => None,
        }
        .unzip();
        drop(cur);

        unsafe { task::ctx::switch_ctx(kframe, new_kframe) };
        ret
    }
}

fn select_cpu(affinity: &crate::cpu::CpuMask) -> Option<usize> {
    affinity.iter_ones().next()
}

/// # Safety
///
/// This function must be called only in task-migrate IPI handlers.
pub unsafe fn task_migrate_handler() {
    loop {
        match MIGRATION_QUEUE[SCHED.cpu].steal_batch(&SCHED.run_queue) {
            Steal::Empty | Steal::Success(_) => break,
            Steal::Retry => {}
        }
    }
}
