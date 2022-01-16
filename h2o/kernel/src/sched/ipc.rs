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
        solvent::Error(match val {
            IpcError::QueueFull(_) => solvent::ENOSPC,
            IpcError::QueueEmpty => solvent::ENOENT,
            IpcError::Task(_) => solvent::ESRCH,
            IpcError::SendChannelClosed(_) => solvent::EPIPE,
            IpcError::ReceiveChannelClosed => solvent::EPIPE,
        })
    }
}
