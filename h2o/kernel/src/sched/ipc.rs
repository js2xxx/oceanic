mod channel;

pub use channel::{Channel, Packet};

use super::task::TaskError;

#[derive(Debug)]
pub enum IpcError {
    QueueFull(Packet),
    QueueEmpty,
    Task(TaskError),
    SendChannelClosed(Packet),
    ReceiveChannelClosed,
}

impl From<IpcError> for solvent::Error {
    fn from(val: IpcError) -> Self {
        match val {
            IpcError::QueueFull(_) => solvent::Error::ENOSPC,
            IpcError::QueueEmpty => solvent::Error::ENOENT,
            IpcError::Task(_) => solvent::Error::ESRCH,
            IpcError::SendChannelClosed(_) => solvent::Error::EPIPE,
            IpcError::ReceiveChannelClosed => solvent::Error::EPIPE,
        }
    }
}
