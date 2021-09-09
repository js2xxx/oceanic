use super::task;
use crate::cpu::time::Instant;
use alloc::vec::Vec;
use archop::IntrMutex;
use canary::Canary;

use alloc::collections::LinkedList;
use core::time::Duration;
use spin::{Lazy, Mutex};

const MINIMUM_TIME_GRANULARITY: Duration = Duration::from_millis(30);

pub static MIGRATION_QUEUE: Lazy<Vec<Mutex<LinkedList<task::Init>>>> = Lazy::new(|| {
      let cnt = crate::cpu::count();
      (0..cnt).map(|_| Mutex::new(LinkedList::new())).collect()
});

#[thread_local]
pub static SCHED: Lazy<IntrMutex<Scheduler>> = Lazy::new(|| {
      IntrMutex::new(Scheduler {
            canary: Canary::new(),
            cpu: unsafe { crate::cpu::id() },
            running: None,
            need_reload: true,
            run_queue: LinkedList::new(),
      })
});

pub struct Scheduler {
      canary: Canary<Scheduler>,
      cpu: usize,
      running: Option<task::Ready>,
      pub(in crate::sched) need_reload: bool,
      run_queue: LinkedList<task::Ready>,
}

impl Scheduler {
      pub fn push(&mut self, task: task::Init) {
            self.canary.assert();

            let affinity = {
                  let ti_map = task::tid::TI_MAP.lock();
                  let ti = ti_map.get(&task.tid()).expect("Invalid init");
                  ti.affinity()
            };

            if !affinity.get(self.cpu).map_or(false, |r| *r) {
                  let cpu = select_cpu(&affinity).expect("Zero affinity");
                  MIGRATION_QUEUE[cpu].lock().push_back(task);

                  unsafe { crate::cpu::arch::apic::ipi::task_migrate(cpu) };
            } else {
                  let time_slice = MINIMUM_TIME_GRANULARITY;
                  let task = task::Ready::from_init(task, self.cpu, time_slice);
                  self.run_queue.push_back(task);
            }
      }

      pub fn current(&self) -> Option<&task::Ready> {
            self.canary.assert();

            self.running.as_ref()
      }

      pub fn current_mut(&mut self) -> Option<&mut task::Ready> {
            self.canary.assert();

            self.running.as_mut()
      }

      pub fn unblock(&mut self, task: task::Blocked) {
            self.canary.assert();

            let time_slice = MINIMUM_TIME_GRANULARITY;
            let task = task::Ready::from_blocked(task, time_slice);
            self.run_queue.push_back(task);
      }

      fn update(&mut self, cur_time: Instant) -> bool {
            self.canary.assert();

            let sole = self.run_queue.is_empty();
            let cur = match self.current_mut() {
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
                  | task::RunningState::Dying(_)
                  | task::RunningState::Drowsy(..) => true,
            }
      }

      pub fn tick(&mut self, cur_time: Instant) {
            let need_resched = self.update(cur_time);

            if need_resched {
                  self.schedule(cur_time);
            }
      }

      fn schedule(&mut self, cur_time: Instant) -> bool {
            self.canary.assert();

            let cur_cpu = self.cpu;
            if self.run_queue.is_empty() {
                  return false;
            }

            let mut next = self.run_queue.pop_front().unwrap();
            next.running_state = task::RunningState::Running(cur_time);
            next.cpu = cur_cpu;

            if let Some(mut prev) = self.running.replace(next) {
                  match &prev.running_state {
                        task::RunningState::NeedResched => {
                              prev.running_state = task::RunningState::NotRunning;
                              self.run_queue.push_back(prev);
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

            self.need_reload = true;
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
      let mut sched = SCHED.lock();
      let mut mq = MIGRATION_QUEUE[sched.cpu].lock();
      while let Some(task) = mq.pop_front() {
            sched.push(task);
      }
}
