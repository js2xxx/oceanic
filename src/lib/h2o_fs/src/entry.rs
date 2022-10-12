use core::any::Any;

use solvent_rpc::io::{entry::EntryServer, Error, Metadata, OpenOptions};
use solvent_std::{path::Path, sync::Arsc};

pub trait Entry: IntoAny + Send + Sync + 'static {
    fn open(
        self: Arsc<Self>,
        path: &Path,
        options: OpenOptions,
        conn: EntryServer,
    ) -> Result<(), Error>;

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