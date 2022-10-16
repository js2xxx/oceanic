use solvent::ipc::Channel;

use super::*;

#[derive(SerdePacket, Debug, Clone)]
pub struct Metadata {
    pub file_type: FileType,
    pub perm: Permission,
    pub len: usize,
}

#[derive(SerdePacket, Debug, Copy, Clone, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
}

#[protocol]
pub trait Entry: crate::core::Cloneable + crate::core::Closeable {
    fn open(path: PathBuf, options: OpenOptions, conn: Channel) -> Result<(), Error>;

    fn metadata() -> Result<Metadata, Error>;
}
