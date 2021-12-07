mod channel;
mod msg;

pub use channel::Channel;
pub use msg::Message;

use super::task::TaskError;

#[derive(Debug)]
pub enum IpcError<T> {
    QueueFull(T),
    QueueEmpty,
    Task(TaskError),
    ChannelClosed(T),
}
