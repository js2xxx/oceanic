use alloc::sync::Arc;

use super::{hdl::HandleMap, Tid};
use crate::mem;

#[derive(Debug)]
pub struct Space {
    mem: Arc<mem::space::Space>,
    handles: HandleMap,
}

impl Space {
    pub fn new(ty: super::Type) -> Arc<Self> {
        Arc::new(Space {
            mem: mem::space::Space::new(ty),
            handles: HandleMap::new(),
        })
    }

    pub fn new_current() -> Arc<Self> {
        Arc::new(Space {
            mem: mem::space::with_current(Arc::clone),
            handles: HandleMap::new(),
        })
    }

    #[inline]
    pub fn mem(&self) -> &Arc<mem::space::Space> {
        &self.mem
    }

    #[inline]
    pub fn handles(&self) -> &HandleMap {
        &self.handles
    }

    pub fn child(&self, hdl: sv_call::Handle) -> sv_call::Result<Tid> {
        super::PREEMPT.scope(|| self.handles().get::<Tid>(hdl).map(|w| Tid::clone(w)))
    }
}
