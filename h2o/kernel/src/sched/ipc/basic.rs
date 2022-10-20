use sv_call::Feature;

use super::*;
use crate::sched::task::hdl::DefaultFeature;

#[derive(Debug, Default)]
pub struct BasicEvent {
    event_data: EventData,
}

impl BasicEvent {
    #[inline]
    pub fn new(init_signal: usize) -> Arc<Self> {
        Arc::new(BasicEvent {
            event_data: EventData::new(init_signal),
        })
    }
}

impl Event for BasicEvent {
    #[inline]
    fn event_data(&self) -> &EventData {
        &self.event_data
    }
}

unsafe impl DefaultFeature for BasicEvent {
    #[inline]
    fn default_features() -> sv_call::Feature {
        Feature::SEND | Feature::SYNC | Feature::WAIT | Feature::EXECUTE
    }
}
