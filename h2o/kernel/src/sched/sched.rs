pub mod deque;
pub mod epoch;

use super::task;
use crate::cpu::time::Instant;
use alloc::vec::Vec;
use archop::{IntrMutex, IntrMutexGuard};
use canary::Canary;
use deque::{Injector, Steal, Worker};

use core::time::Duration;
use spin::Lazy;

const MINIMUM_TIME_GRANULARITY: Duration = Duration::from_millis(30);

pub static MIGRATION_QUEUE: Lazy<Vec<Injector<task::Ready>>> = Lazy::new(|| {
      let count = crate::cpu::count();
      core::iter::repeat_with(Injector::new).take(count).collect()
});

#[thread_local]
pub static SCHED: Lazy<Scheduler> = Lazy::new(|| Scheduler {
      canary: Canary::new(),
      cpu: unsafe { crate::cpu::id() },
      run_state: IntrMutex::new(RunState {
            current: None,
            need_reload: true,
      }),
      run_queue: Worker::new_fifo(),
});

pub struct RunState {
      pub(in crate::sched) current: Option<task::Ready>,
      pub(in crate::sched) need_reload: bool,
}

pub struct Scheduler {
      canary: Canary<Scheduler>,
      cpu: usize,
      run_queue: Worker<task::Ready>,
      pub(in crate::sched) run_state: IntrMutex<RunState>,
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
            let mut rs = self.run_state.lock();
            rs.current.as_mut().map(func)
      }

      pub fn unblock(&self, task: task::Blocked) {
            self.canary.assert();

            let time_slice = MINIMUM_TIME_GRANULARITY;
            let task = task::Ready::from_blocked(task, time_slice);
            if task.cpu == self.cpu {
                  self.run_queue.push(task);
            } else {
                  let cpu = task.cpu;
                  MIGRATION_QUEUE[cpu].push(task);
                  unsafe { crate::cpu::arch::apic::ipi::task_migrate(cpu) };
            }
      }

      fn update(&self, cur_time: Instant, rs: &mut IntrMutexGuard<RunState>) -> bool {
            self.canary.assert();

            let sole = self.run_queue.is_empty();
            let cur = match rs.current.as_mut() {
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
                  task::RunningState::NeedResched
                  | task::RunningState::Dying(..)
                  | task::RunningState::Drowsy(..) => true,
            }
      }

      pub fn tick(&self, cur_time: Instant) {
            let mut rs = self.run_state.lock();
            let need_resched = self.update(cur_time, &mut rs);

            if need_resched {
                  self.schedule(cur_time, &mut rs);
            }
      }

      fn schedule(&self, cur_time: Instant, rs: &mut IntrMutexGuard<RunState>) -> bool {
            self.canary.assert();

            let cur_cpu = self.cpu;
            if self.run_queue.is_empty() {
                  return false;
            }

            let mut next = self.run_queue.pop().unwrap();
            next.running_state = task::RunningState::Running(cur_time);
            next.cpu = cur_cpu;

            if let Some(mut prev) = rs.current.replace(next) {
                  match &prev.running_state {
                        task::RunningState::NeedResched => {
                              prev.running_state = task::RunningState::NotRunning;
                              self.run_queue.push(prev);
                        }
                        task::RunningState::Drowsy(..) => {
                              task::Ready::into_blocked(prev);
                        }
                        task::RunningState::Dying(..) => {
                              task::destroy(task::Ready::into_dead(prev));
                        }
                        _ => unreachable!(),
                  }
            }

            rs.need_reload = true;
            true
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
