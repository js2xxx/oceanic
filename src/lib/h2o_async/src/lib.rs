#![no_std]

pub mod dev;
pub mod disp;
pub mod ipc;
pub mod time;
mod utils;

extern crate alloc;

pub use self::disp::dispatch;

pub mod test {
    use alloc::boxed::Box;
    use core::{
        future::Future,
        task::{Context, Poll},
    };

    use futures::task::noop_waker_ref;
    use solvent::{
        error::{Result, EPIPE},
        ipc::Packet,
        prelude::Handle,
    };
    use solvent_std::thread::{self, Backoff};

    use crate::disp::{DispReceiver, DispSender};

    const NUM_PACKET: usize = 2000;

    fn test_tx(tx: DispSender) -> (impl FnOnce(), impl Future<Output = ()>) {
        let (i1, i2) = solvent::ipc::Channel::new();
        let i1 = crate::ipc::Channel::new(i1, tx.clone());
        let i2 = crate::ipc::Channel::new(i2, tx);

        let recv = async move {
            let mut packet = Packet {
                buffer: alloc::vec![0; 4],
                handles: alloc::vec![Handle::NULL; 4],
                ..Default::default()
            };
            for index in 0..NUM_PACKET {
                // log::debug!("\t\t\tReceive #{index}");
                i2.receive_packet(&mut packet)
                    .await
                    .expect("Failed to receive packet");
                // log::debug!("\t\t\tGot");
                assert_eq!(packet.buffer[0], index as u8);
            }
            // log::debug!("\t\t\tReceive finished");
        };

        let send = move || {
            let mut packet = Packet {
                id: Some(0),
                buffer: alloc::vec![0],
                ..Default::default()
            };
            for index in 0..NUM_PACKET {
                // log::debug!("Send #{index}");
                packet.buffer.resize(1, index as u8);
                // thread::yield_now();
                i1.send_packet(&mut packet).expect("Failed to send packet");
            }
            // log::debug!("Send finished");
        };

        (send, recv)
    }

    fn test_rx(rx: DispReceiver) {
        let backoff = Backoff::new();
        loop {
            match rx.poll_receive() {
                Poll::Ready(res) => match res {
                    Ok(()) => {}
                    Err(EPIPE) => break,
                    Err(err) => log::warn!("Error while polling for dispatcher: {:?}", err),
                },
                Poll::Pending => {}
            }
            backoff.snooze()
        }
    }

    pub fn test_disp() -> Result {
        log::debug!("Has {} cpus available", solvent::task::cpu_num());
        let (tx, rx) = crate::dispatch(10);
        let j = thread::spawn(move || {
            let backoff = Backoff::new();

            let (send, recv) = test_tx(tx);
            let mut fut = Box::pin(recv);
            let mut cx = Context::from_waker(noop_waker_ref());

            let s = thread::spawn(send);
            while fut.as_mut().poll(&mut cx).is_pending() {
                backoff.snooze()
            }
            s.join()
        });

        test_rx(rx);

        j.join();
        Ok(())
    }
}
