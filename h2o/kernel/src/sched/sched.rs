pub mod deque;
pub mod epoch;
pub mod preempt;

use alloc::vec::Vec;
use core::time::Duration;

use canary::Canary;
use deque::{Injector, Steal, Worker};
pub use preempt::{PreemptGuard, PreemptMutex};
use spin::Lazy;

use super::task;
use crate::cpu::time::Instant;

const MINIMUM_TIME_GRANULARITY: Duration = Duration::from_millis(30);

pub static MIGRATION_QUEUE: Lazy<Vec<Injector<task::Ready>>> = Lazy::new(|| {
    let count = crate::cpu::count();
    core::iter::repeat_with(Injector::new).take(count).collect()
});

#[thread_local]
pub static SCHED: Lazy<Scheduler> = Lazy::new(|| Scheduler {
    canary: Canary::new(),
    cpu: unsafe { crate::cpu::id() },
    current: PreemptMutex::new(),
    run_queue: Worker::new_fifo(),
});

pub struct Scheduler {
    canary: Canary<Scheduler>,
    cpu: usize,
    run_queue: Worker<task::Ready>,
    pub(in crate::sched) current: PreemptMutex,
}

impl Scheduler {
    pub fn push(&self, task: task::Init) {
        self.canary.assert();

        let affinity = task::tid::get(&task.tid())
            .expect("Invalid init")
            .affinity();

        let time_slice = MINIMUM_TIME_GRANULARITY;
        if !affinity.get(self.cpu).map_or(false, |r| *r) {
            let cpu = select_cpu(&affinity).expect("Zero affinity");
            let task = task::Ready::from_init(task, cpu, time_slice);
            MIGRATION_QUEUE[cpu].push(task);

            unsafe { crate::cpu::arch::apic::ipi::task_migrate(cpu) };
        } else {
            let task = task::Ready::from_init(task, self.cpu, time_slice);
            self.run_queue.push(task);
        }
    }

    pub fn with_current<F, R>(&self, func: F) -> Option<R>
    where
        F: FnOnce(&mut task::Ready) -> R,
    {
        let mut cur = self.current.lock();
        cur.as_mut().map(func)
    }

    pub fn try_preempt(&self) {
        self.canary.assert();

        let cur = self.current.lock();
        if let Some(ref cur_ref) = &*cur {
            if cur_ref.preempt_count == 0 {
                drop(cur_ref);
                self.schedule_impl(Instant::now(), cur, |mut task| {
                    task.running_state = task::RunningState::NotRunning;
                    self.run_queue.push(task);
                });
            }
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

        self.schedule_impl(cur_time, cur, |task| {
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
            self.run_queue.push(task);
        } else {
            let cpu = task.cpu;
            MIGRATION_QUEUE[cpu].push(task);
            unsafe { crate::cpu::arch::apic::ipi::task_migrate(cpu) };
        }
    }

    pub fn exit_current(&self, retval: usize) {
        self.canary.assert();
        let cur = self.current.lock();

        self.schedule_impl(Instant::now(), cur, |task| task::Ready::exit(task, retval));
    }

    fn update(&self, cur_time: Instant, cur: &mut PreemptGuard) -> bool {
        self.canary.assert();

        let sole = self.run_queue.is_empty();
        let cur = match cur.as_mut() {
            Some(task) => task,
            None => return !sole,
        };

        match &cur.running_state {
            task::RunningState::Running(start_time) => {
                if cur.time_slice() < cur_time - *start_time && !sole {
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

    pub fn tick(&self, cur_time: Instant) {
        let mut cur = self.current.lock();
        let need_resched = self.update(cur_time, &mut cur);

        if need_resched {
            self.schedule(cur_time, cur);
        }
    }

    fn schedule(&self, cur_time: Instant, cur: PreemptGuard) -> bool {
        self.canary.assert();

        self.schedule_impl(cur_time, cur, |mut task| {
            debug_assert!(matches!(
                task.running_state,
                task::RunningState::NeedResched
            ));
            task.running_state = task::RunningState::NotRunning;
            self.run_queue.push(task);
        })
        .is_some()
    }

    fn schedule_impl<F, R>(&self, cur_time: Instant, mut cur: PreemptGuard, func: F) -> Option<R>
    where
        F: FnOnce(task::Ready) -> R,
    {
        self.canary.assert();

        let cur_cpu = self.cpu;
        let mut next = match self.run_queue.pop() {
            Some(task) => task,
            None => return None,
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
