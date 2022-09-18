#![no_std]

pub mod dev;
pub mod disp;
pub mod exe;
pub mod ipc;
mod utils;

extern crate alloc;

pub use self::disp::dispatch;

pub mod test {
    use core::future::Future;

    use solvent::{ipc::Packet, prelude::Handle, random};

    use crate::disp::DispSender;

    const NUM_PACKET: usize = 2000;

    fn test_tx(tx: DispSender) -> (impl Future<Output = ()>, impl Future<Output = ()>) {
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
                // log::debug!("\t\t\tGot #{index}");
                assert_eq!(packet.buffer[0], index as u8);
            }
            log::debug!("\t\t\tReceive finished");
        };

        let send = async move {
            let mut packet = Packet {
                id: Some(0),
                buffer: alloc::vec![0],
                ..Default::default()
            };
            for index in 0..NUM_PACKET {
                packet.buffer.resize(1, index as u8);
                packet
                    .buffer
                    .extend(core::iter::repeat_with(|| random() as u8).take(199));
                async {
                    // log::debug!("Send #{index}");
                    i1.send_packet(&mut packet).expect("Failed to send packet")
                }
                .await;
            }
            log::debug!("Send finished");
        };

        (send, recv)
    }

    pub fn test_disp() {
        log::debug!("Has {} cpus available", solvent::task::cpu_num());
        let exe = crate::exe::ThreadPool::new(2);
        exe.block_on(|pool| async move {
            let tx = pool.dispatch(10);
            let (send, recv) = test_tx(tx);
            let recv = pool.spawn(recv);
            let send = pool.spawn(send);
            recv.await;
            send.await;
        });
    }
}
