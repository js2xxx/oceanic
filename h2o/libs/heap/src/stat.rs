use core::fmt::Display;

#[derive(Debug, Clone)]
pub struct Stat {
      capacity: usize,
      in_cnt: usize,
      out_cnt: usize,
      current_used: usize,
}

impl Stat {
      pub const fn new() -> Self {
            Stat {
                  capacity: 0,
                  in_cnt: 0,
                  out_cnt: 0,
                  current_used: 0,
            }
      }

      pub fn capacity(&self) -> usize {
            self.capacity
      }
      pub fn in_cnt(&self) -> usize {
            self.in_cnt
      }
      pub fn out_cnt(&self) -> usize {
            self.out_cnt
      }
      pub fn current_used(&self) -> usize {
            self.current_used
      }

      pub fn extend(&mut self, size: usize) {
            self.capacity += size;
      }

      pub fn alloc(&mut self, size: usize) {
            self.out_cnt += size;
            self.current_used += size;
      }

      pub fn dealloc(&mut self, size: usize) {
            self.in_cnt += size;
            self.current_used -= size;
      }
}

impl Display for Stat {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            writeln!(f, "Statistics of heap (measured in bytes):")?;
            writeln!(f, "\tCapacity: {:#x}", self.capacity)?;
            writeln!(f, "\tAmount of all imported: {:#x}", self.in_cnt)?;
            writeln!(f, "\tAmount of all exported: {:#x}", self.out_cnt)?;
            writeln!(f, "\tCurrent used memory: {:#x}", self.current_used)
      }
}
