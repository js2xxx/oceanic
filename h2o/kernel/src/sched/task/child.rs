use super::Tid;
use crate::sched::wait::WaitCell;

#[derive(Debug)]
pub struct Child {
    cell: WaitCell<usize>,
    tid: Tid,
}

impl Child {
    pub fn new(tid: Tid) -> Self {
        Child {
            cell: WaitCell::new(),
            tid,
        }
    }

    pub fn cell(&self) -> &WaitCell<usize> {
        &self.cell
    }

    pub fn tid(&self) -> &Tid {
        &self.tid
    }
}
