use alloc::sync::Arc;

use super::Tid;
use crate::sched::wait::WaitCell;

#[derive(Debug, Clone)]
pub struct Child {
    cell: Arc<WaitCell<usize>>,
    tid: Tid,
}

impl Child {
    pub fn new(tid: Tid) -> Self {
        Child {
            cell: Arc::new(WaitCell::new()),
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
