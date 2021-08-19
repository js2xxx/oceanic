use super::task;
use crate::cpu::time::Instant;

use alloc::collections::LinkedList;
use core::time::Duration;

const MINIMUM_TIME_GRANULARITY: Duration = Duration::from_millis(30);

pub struct Scheduler {
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
            let time_slice = MINIMUM_TIME_GRANULARITY;
            let task = task::Ready::from_init(task, self.cpu, time_slice);
            self.list.push_back(task);
      }

      pub fn current(&self) -> Option<&task::Ready> {
            self.list.front()
      }

      pub fn current_mut(&mut self) -> Option<&mut task::Ready> {
            self.list.front_mut()
      }

      fn update(&mut self, cur_time: Instant) {
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
            let cur_cpu = self.cpu;
            if self.list.len() <= 1 { return; }

            let mut prev = self.list.pop_front().unwrap();
            let next = self.current_mut().unwrap();

            prev.running_state = task::RunningState::NotRunning;
            next.running_state = task::RunningState::Running(cur_time);
            next.cpu = cur_cpu;

            // TODO: further switches

            self.list.push_back(prev);
      }
}
