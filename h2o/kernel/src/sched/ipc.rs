mod channel;

pub use channel::{Channel, Packet};

use super::task::TaskError;

#[derive(Debug)]
pub enum IpcError {
    QueueFull(Packet),
    QueueEmpty,
    Task(TaskError),
    ChannelClosed(Packet),
}

impl Into<solvent::Error> for IpcError {
    fn into(self) -> solvent::Error {
        solvent::Error(match self {
            IpcError::QueueFull(_) => solvent::ENOSPC,
            IpcError::QueueEmpty => solvent::ENOENT,
            IpcError::Task(_) => solvent::ESRCH,
            IpcError::ChannelClosed(_) => solvent::EPIPE,
        })
    }
}
