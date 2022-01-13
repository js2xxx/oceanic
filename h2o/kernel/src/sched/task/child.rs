use alloc::sync::Arc;

use spin::Mutex;

use super::Tid;
use crate::sched::{ipc::Channel, wait::WaitCell};

#[derive(Debug, Clone)]
pub struct Child {
    cell: Arc<WaitCell<usize>>,
    excep_chan: Arc<Mutex<Option<Channel>>>,
    tid: Tid,
}

impl Child {
    pub fn new(tid: Tid) -> Self {
        Child {
            cell: Arc::new(WaitCell::new()),
            excep_chan: Arc::new(Mutex::new(None)),
            tid,
        }
    }

    #[inline]
    pub fn cell(&self) -> &WaitCell<usize> {
        &self.cell
    }

    #[inline]
    pub fn excep_chan(&self) -> Arc<Mutex<Option<Channel>>> {
        Arc::clone(&self.excep_chan)
    }

    #[inline]
    pub fn tid(&self) -> &Tid {
        &self.tid
    }
}
