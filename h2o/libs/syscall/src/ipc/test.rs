use core::ptr;

use super::*;
use crate::{call::*, Error};

pub fn test(stack: (*mut u8, *mut u8, Handle)) {
    #[inline]
    fn rp(id: usize, hdl: &mut [Handle], buf: &mut [u8]) -> RawPacket {
        RawPacket {
            id,
            handles: hdl.as_mut_ptr(),
            handle_count: hdl.len(),
            handle_cap: hdl.len(),
            buffer: buf.as_mut_ptr(),
            buffer_size: buf.len(),
            buffer_cap: buf.len(),
        }
    }

    let mut c1 = Handle::NULL;
    let mut c2 = Handle::NULL;
    chan_new(&mut c1, &mut c2).expect("Failed to create a channel");
    let (c1, c2) = (c1, c2);

    // Test in 1 task (transfering to myself).
    let e = {
        let e = event_new(false).expect("Failed to create a event");
        event_notify(e, u8::MAX).expect("Failed to notify the event");

        // Sending

        let mut buf = [1u8, 2, 3, 4, 5, 6, 7];
        let mut hdl = [e];

        let sendee = rp(100, &mut hdl, &mut buf);
        chan_send(c1, &sendee).expect("Failed to send a packet into the channel");

        {
            // Null handles can't be sent.
            hdl[0] = Handle::NULL;
            let mut sendee = rp(0, &mut hdl, &mut buf);
            let ret = chan_send(c1, &sendee);
            assert_eq!(ret, Err(Error::EINVAL));

            // The channel itself can't be sent.
            // To make connections to other tasks, use `init_chan`.
            hdl[0] = c1;
            sendee = rp(0, &mut hdl, &mut buf);
            let ret = chan_send(c1, &sendee);
            assert_eq!(ret, Err(Error::EPERM));

            // Neither can its peer.
            hdl[0] = c2;
            sendee = rp(0, &mut hdl, &mut buf);
            let ret = chan_send(c1, &sendee);
            assert_eq!(ret, Err(Error::EPERM));
        }

        {
            let mut receivee = rp(0, &mut [], &mut buf);
            let ret = chan_recv(c2, &mut receivee, u64::MAX);
            assert_eq!(ret, Err(Error::EBUFFER));

            receivee = rp(0, &mut hdl, &mut []);
            let ret = chan_recv(c2, &mut receivee, u64::MAX);
            assert_eq!(ret, Err(Error::EBUFFER));
        }

        buf.fill(0);
        let mut receivee = rp(0, &mut hdl, &mut buf);
        chan_recv(c2, &mut receivee, u64::MAX)
            .expect("Failed to receive a packet from the channel");
        assert_eq!(buf, [1u8, 2, 3, 4, 5, 6, 7]);
        assert_eq!(receivee.id, 100);

        let e = hdl[0];
        event_notify(e, u8::MAX).expect("Failed to notify the event");

        receivee = rp(0, &mut hdl, &mut buf);
        let ret = chan_recv(c2, &mut receivee, 0);
        assert_eq!(ret, Err(Error::ENOENT));

        e
    };

    // Multiple tasks.
    {
        extern "C" fn func(init_chan: Handle) {
            ::log::trace!("func here: {:?}", init_chan);
            let mut buf = [0; 7];
            let mut hdl = [Handle::NULL];
            let mut p = rp(0, &mut hdl, &mut buf);

            chan_recv(init_chan, &mut p, u64::MAX).expect("Failed to receive the init packet");
            assert_eq!(p.id, 200);
            for b in buf.iter_mut() {
                *b += 5;
            }
            event_notify(hdl[0], u8::MAX).expect("Failed to notify the e in func");
            ::log::trace!("Responding");
            p.id = 200;
            chan_send(init_chan, &p).expect("Failed to send the response");

            ::log::trace!("Finished");
            crate::task::exit(Ok(()));
        }

        let other = {
            let ci = crate::task::ExecInfo {
                name: ptr::null_mut(),
                name_len: 0,
                space: Handle::NULL,
                entry: func as *mut u8,
                stack: stack.0,
                init_chan: c2,
                arg: 0,
            };

            task_exec(&ci).expect("Failed to create task other")
        };

        let mut buf = [1u8, 2, 3, 4, 5, 6, 7];
        let mut hdl = [e];

        ::log::trace!("Sending the initial packet");
        let mut p = rp(200, &mut hdl, &mut buf);
        let id = chan_csend(c1, &p).expect("Failed to send init packet");
        assert_eq!(id, 200);

        p.id = 0;
        ::log::trace!("Receiving the response");
        chan_crecv(c1, id, &mut p, u64::MAX).expect("Failed to receive the response");
        assert_eq!(p.id, 200);
        assert_eq!(buf, [6, 7, 8, 9, 10, 11, 12]);

        ::log::trace!("Finished");
        let e = hdl[0];
        event_notify(e, u8::MAX).expect("Failed to notify the event in master");
        obj_drop(e).expect("Failed to drop the event in master");

        task_join(other).expect("Failed to join the task");
    }

    mem_unmap(Handle::NULL, stack.1).expect("Failed to unmap the memory");
    obj_drop(stack.2).expect("Failed to deallocate the stack memory");
}
