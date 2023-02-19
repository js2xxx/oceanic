use core::task::Poll;

use solvent_core::sync::channel::{oneshot, TryRecvError};

/// Check if the channel has already received a result from previous polling (or
/// dispatcher's event).
#[allow(dead_code)]
pub(crate) fn simple_recv<T>(result: &mut Option<oneshot::Receiver<T>>) -> Option<Poll<T>> {
    if let Some(rx) = result.take() {
        match rx.try_recv() {
            // Has a result, return it
            Ok(res) => return Some(Poll::Ready(res)),

            // Not yet, continue waiting
            Err(TryRecvError::Empty) => {
                *result = Some(rx);
                return Some(Poll::Pending);
            }

            // Channel early disconnected, restart the default process
            Err(TryRecvError::Disconnected) => {}
        };
    }
    None
}
