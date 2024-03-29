#![no_std]
#![feature(control_flow_enum)]
#![feature(error_in_core)]

pub mod dev;
pub mod disp;
pub mod exe;
pub mod io;
pub mod ipc;
pub mod mem;
pub mod sync;
pub mod time;
mod utils;

extern crate alloc;

#[cfg(feature = "runtime")]
pub use self::exe::runtime::*;

#[cfg(feature = "runtime")]
pub mod test {
    use core::future::Future;

    use futures_lite::future::{yield_now, zip};
    use solvent::{
        ipc::Packet,
        prelude::{Handle, PhysOptions},
        random,
    };

    const NUM_PACKET: usize = 2000;

    fn test_tx() -> (impl Future<Output = ()>, impl Future<Output = ()>) {
        let (i1, i2) = solvent::ipc::Channel::new();
        let i1 = crate::ipc::Channel::new(i1);
        let i2 = crate::ipc::Channel::new(i2);

        let recv = async move {
            let mut packet = Packet {
                buffer: alloc::vec![0; 4],
                handles: alloc::vec![Handle::NULL; 4],
                ..Default::default()
            };
            for index in 0..NUM_PACKET {
                // log::debug!("\t\t\tReceive #{index}");
                i2.receive(&mut packet)
                    .await
                    .expect("Failed to receive packet");
                assert_eq!(packet.buffer[0], index as u8);
            }
            log::debug!("\t\t\tReceive finished");
        };

        let send = async move {
            let mut packet = Packet {
                id: None,
                buffer: alloc::vec![0],
                ..Default::default()
            };
            for index in 0..NUM_PACKET {
                packet.buffer.resize(1, index as u8);
                packet
                    .buffer
                    .extend(core::iter::repeat_with(|| random() as u8).take(199));
                // log::debug!("Send #{index}");
                i1.send(&mut packet).expect("Failed to send packet");
                if index % 10 == 5 {
                    yield_now().await
                }
            }
            log::debug!("Send finished");
        };

        (send, recv)
    }

    async fn test_stream() {
        let phys = solvent::mem::Phys::allocate(5, PhysOptions::ZEROED | PhysOptions::RESIZABLE)
            .expect("Failed to allocate memory");
        let stream =
            unsafe { crate::io::Stream::new(solvent_core::io::RawStream { phys, seeker: 0 }) };
        stream.write(&[1, 2, 3, 4, 5, 6, 7]).await.unwrap();
        stream
            .seek(solvent_core::io::SeekFrom::Current(-4))
            .await
            .unwrap();
        let mut buf = [0; 10];
        let len = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..len], [4, 5, 6, 7]);
    }

    pub async fn test_disp() {
        log::debug!("Has {} cpus available", solvent::task::cpu_num());

        test_stream().await;

        let (send, recv) = test_tx();
        let recv = crate::spawn(recv);
        let send = crate::spawn_local(send);
        zip(send, recv).await;
    }
}
