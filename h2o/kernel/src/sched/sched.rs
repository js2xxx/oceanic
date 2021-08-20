use super::task;
use super::task::ctx;
use crate::cpu::time::Instant;
use canary::Canary;

use alloc::collections::LinkedList;
use core::time::Duration;
use spin::{Lazy, Mutex};

const MINIMUM_TIME_GRANULARITY: Duration = Duration::from_millis(30);

#[thread_local]
pub static SCHED: Lazy<Mutex<Scheduler>> = Lazy::new(|| {
      Mutex::new(Scheduler {
            canary: Canary::new(),
            cpu: unsafe { crate::cpu::id() },
            list: LinkedList::new(),
      })
});

pub struct Scheduler {
      canary: Canary<Scheduler>,
      cpu: usize,
      list: LinkedList<task::Ready>,
}

impl Scheduler {
      /// Push a task into the scheduler's run queue.
      ///
      /// # Safety
      ///
      /// The caller must ensure that the affinity of the task contains the scheduler's CPU.
      pub unsafe fn push(&mut self, task: task::Init) {
            self.canary.assert();

            let time_slice = MINIMUM_TIME_GRANULARITY;
            let task = task::Ready::from_init(task, self.cpu, time_slice);
            self.list.push_back(task);
      }

      pub fn current(&self) -> Option<&task::Ready> {
            self.canary.assert();

            self.list.front()
      }

      pub fn current_mut(&mut self) -> Option<&mut task::Ready> {
            self.canary.assert();

            self.list.front_mut()
      }

      fn update(&mut self, cur_time: Instant) {
            self.canary.assert();

            let len = self.list.len();
            let cur = match self.current_mut() {
                  Some(task) => task,
                  None => return,
            };

            let start_time = match cur.running_state {
                  task::RunningState::Running(t) => t,
                  task::RunningState::NotRunning => panic!("Not running"),
                  task::RunningState::NeedResched => return,
            };

            if cur.time_slice() < cur_time - start_time && len > 1 {
                  cur.running_state = task::RunningState::NeedResched;
            }
      }

      pub fn tick(&mut self, cur_time: Instant) {
            self.update(cur_time);

            if self.current_mut().map_or(false, |cur| {
                  matches!(cur.running_state, task::RunningState::NeedResched)
            }) {
                  self.schedule(cur_time);
            }
      }

      fn schedule(&mut self, cur_time: Instant) {
            self.canary.assert();

            let cur_cpu = self.cpu;
            if self.list.len() <= 1 {
                  return;
            }

            let mut prev = self.list.pop_front().unwrap();
            let next = self.current_mut().unwrap();

            prev.running_state = task::RunningState::NotRunning;
            next.running_state = task::RunningState::Running(cur_time);
            next.cpu = cur_cpu;

            // TODO: further switches

            self.list.push_back(prev);
      }

      /// Restore the context of the current task.
      ///
      /// # Safety
      ///
      /// This function should only be called at the end of interrupt / syscall handlers.
      pub unsafe fn restore_current(
            &mut self,
            frame: *const ctx::arch::Frame,
      ) -> *const ctx::arch::Frame {
            self.current().map_or(frame, |cur| {
                  unsafe { cur.space().load() };
                  unsafe { cur.get_arch_context() }
            })
      }
}
