use super::Arsc;

mod blocking;
pub mod oneshot_;

#[derive(Debug)]
pub struct RecvError;

#[derive(Debug)]
pub enum TryRecvError {
    /// This **channel** is currently empty, but the **Sender**(s) have not yet
    /// disconnected, so data may yet become available.
    Empty,

    /// The **channel**'s sending half has become disconnected, and there will
    /// never be any more data received on it.
    Disconnected,
}

#[inline]
#[must_use]
pub fn oneshot<T>() -> (oneshot_::Sender<T>, oneshot_::Receiver<T>) {
    let packet = Arsc::new(oneshot_::Packet::new());
    (
        oneshot_::Sender::new(Arsc::clone(&packet)),
        oneshot_::Receiver::new(packet),
    )
}
