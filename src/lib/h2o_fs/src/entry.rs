use core::any::Any;

use solvent::prelude::Channel;
use solvent_core::{path::Path, sync::Arsc};
use solvent_rpc::io::{Error, Metadata, OpenOptions};

pub trait Entry: IntoAny + Send + Sync + 'static {
    fn open(
        self: Arsc<Self>,
        path: &Path,
        options: OpenOptions,
        conn: Channel,
    ) -> Result<(), Error>;

    fn metadata(&self) -> Result<Metadata, Error>;

    fn set_metadata(&self, metadata: Metadata) -> Result<(), Error>;
}

pub trait IntoAny: Any {
    fn into_any(self: Arsc<Self>) -> Arsc<dyn Any + Send + Sync>;
}

impl<T: Any + Send + Sync> IntoAny for T {
    fn into_any(self: Arsc<Self>) -> Arsc<dyn Any + Send + Sync> {
        self as _
    }
}
