use core::any::Any;

use solvent::prelude::Channel;
use solvent_core::{path::Path, sync::Arsc};
use solvent_rpc::io::{Error, Metadata, OpenOptions};

use crate::dir::EventTokens;

pub trait Entry: IntoAny + Send + Sync + 'static {
    fn open(
        self: Arsc<Self>,
        tokens: EventTokens,
        path: &Path,
        options: OpenOptions,
        conn: Channel,
    ) -> Result<bool, Error>;

    fn metadata(&self) -> Result<Metadata, Error>;
}

pub trait IntoAny: Any {
    fn into_any(self: Arsc<Self>) -> Arsc<dyn Any + Send + Sync>;
}

impl<T: Any + Send + Sync> IntoAny for T {
    fn into_any(self: Arsc<Self>) -> Arsc<dyn Any + Send + Sync> {
        self as _
    }
}
