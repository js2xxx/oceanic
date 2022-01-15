mod channel;
mod obj;

pub use channel::{Channel, Packet};
pub use obj::Object;

use super::task::TaskError;

#[derive(Debug)]
pub enum IpcError {
    QueueFull(Packet),
    QueueEmpty,
    Task(TaskError),
    ChannelClosed(Packet),
}

impl From<IpcError> for solvent::Error {
    fn from(val: IpcError) -> Self {
        solvent::Error(match val {
            IpcError::QueueFull(_) => solvent::ENOSPC,
            IpcError::QueueEmpty => solvent::ENOENT,
            IpcError::Task(_) => solvent::ESRCH,
            IpcError::ChannelClosed(_) => solvent::EPIPE,
        })
    }
}
