use core::ptr::{self, NonNull};

use solvent::prelude::Virt;
use sv_call::{ipc::*, task::DEFAULT_STACK_SIZE, *};

pub unsafe fn test(virt: &Virt, stack: (*mut u8, *mut u8, Handle)) {
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
    sv_chan_new(&mut c1, &mut c2)
        .into_res()
        .expect("Failed to create a channel");
    let (c1, c2) = (c1, c2);

    // Test in 1 task (transfering to myself).
    let e = {
        let e = sv_int_new(12345)
            .into_res()
            .expect("Failed to create a event");
        assert_eq!(sv_int_get(e).into_res(), Ok(12345));

        // Sending

        let mut buf = [1u8, 2, 3, 4, 5, 6, 7];
        let mut hdl = [e];

        let sendee = rp(100, &mut hdl, &mut buf);
        sv_chan_send(c1, &sendee)
            .into_res()
            .expect("Failed to send a packet into the channel");

        {
            // Null handles can't be sent.
            hdl[0] = Handle::NULL;
            let mut sendee = rp(0, &mut hdl, &mut buf);
            let ret = sv_chan_send(c1, &sendee);
            assert_eq!(ret.into_res(), Err(EINVAL));

            // The channel itself can't be sent.
            // To make connections to other tasks, use `init_chan`.
            hdl[0] = c1;
            sendee = rp(0, &mut hdl, &mut buf);
            let ret = sv_chan_send(c1, &sendee);
            assert_eq!(ret.into_res(), Err(EPERM));

            // Neither can its peer.
            hdl[0] = c2;
            sendee = rp(0, &mut hdl, &mut buf);
            let ret = sv_chan_send(c1, &sendee);
            assert_eq!(ret.into_res(), Err(EPERM));
        }

        {
            let mut receivee = rp(0, &mut [], &mut buf);
            sv_obj_wait(c2, u64::MAX, true, false, SIG_READ)
                .into_res()
                .expect("Failed to wait for the channel");
            let ret = sv_chan_recv(c2, &mut receivee);
            assert_eq!(ret.into_res(), Err(EBUFFER));

            receivee = rp(0, &mut hdl, &mut []);
            let ret = sv_chan_recv(c2, &mut receivee);
            assert_eq!(ret.into_res(), Err(EBUFFER));
        }

        buf.fill(0);
        let mut receivee = rp(0, &mut hdl, &mut buf);
        sv_chan_recv(c2, &mut receivee)
            .into_res()
            .expect("Failed to receive a packet from the channel");
        assert_eq!(buf, [1u8, 2, 3, 4, 5, 6, 7]);
        assert_eq!(receivee.id, 100);

        let e = hdl[0];
        assert_eq!(sv_int_get(e).into_res(), Ok(12345));

        receivee = rp(0, &mut hdl, &mut buf);
        let ret = sv_chan_recv(c2, &mut receivee);
        assert_eq!(ret.into_res(), Err(ENOENT));

        e
    };

    // Multiple tasks.
    {
        const MSG_ID: usize = 123;
        unsafe extern "C" fn func(init_chan: Handle) {
            ::log::trace!("func here: {:?}", init_chan);
            let mut buf = [0; 7];
            let mut hdl = [Handle::NULL];
            let mut p = rp(0, &mut hdl, &mut buf);

            sv_obj_wait(init_chan, u64::MAX, true, false, SIG_READ)
                .into_res()
                .expect("Failed to wait for the channel");
            sv_chan_recv(init_chan, &mut p)
                .into_res()
                .expect("Failed to receive the init packet");
            assert_eq!(p.id, MSG_ID);
            for b in buf.iter_mut() {
                *b += 5;
            }
            assert_eq!(sv_int_get(hdl[0]).into_res(), Ok(12345));
            ::log::trace!("Responding");
            p.id = MSG_ID;
            sv_chan_send(init_chan, &p)
                .into_res()
                .expect("Failed to send the response");

            ::log::trace!("Finished");
            sv_task_exit(0, false)
                .into_res()
                .expect("Failed to exit the task");
        }

        let other = {
            let ci = sv_call::task::ExecInfo {
                name: ptr::null_mut(),
                name_len: 0,
                space: Handle::NULL,
                entry: func as *mut u8,
                stack: stack.0,
                init_chan: c2,
                arg: 0,
            };

            sv_task_exec(&ci)
                .into_res()
                .expect("Failed to create task other")
        };

        let mut buf = [1u8, 2, 3, 4, 5, 6, 7];
        let mut hdl = [e];

        ::log::trace!("Sending the initial packet");
        let mut p = rp(MSG_ID, &mut hdl, &mut buf);
        sv_chan_send(c1, &p)
            .into_res()
            .expect("Failed to send init packet");

        p.id = 0;
        ::log::trace!("Waiting for the initial response");
        sv_obj_wait(c1, u64::MAX, true, false, SIG_READ)
            .into_res()
            .expect("Failed to wait for the channel");
        ::log::trace!("Receiving the response");
        sv_chan_recv(c1, &mut p)
            .into_res()
            .expect("Failed to receive the response");
        assert_eq!(p.id, MSG_ID);
        assert_eq!(buf, [6, 7, 8, 9, 10, 11, 12]);

        ::log::trace!("Finished");
        let e = hdl[0];
        assert_eq!(sv_int_get(e).into_res(), Ok(12345));
        sv_obj_drop(e)
            .into_res()
            .expect("Failed to drop the event in master");

        let mut retval = Default::default();
        sv_obj_wait(other, u64::MAX, true, false, SIG_READ)
            .into_res()
            .expect("Failed to wait for the task");
        sv_task_join(other, &mut retval)
            .into_res()
            .expect("Failed to join the task");
    }

    virt.unmap(NonNull::new_unchecked(stack.1), DEFAULT_STACK_SIZE, false)
        .expect("Failed to unmap the memory");
    sv_obj_drop(stack.2)
        .into_res()
        .expect("Failed to deallocate the stack memory");
}
