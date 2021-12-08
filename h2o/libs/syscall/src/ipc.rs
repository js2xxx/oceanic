use crate::Handle;

#[derive(Debug)]
#[repr(C)]
pub struct RawPacket {
    pub handles: *mut Handle,
    pub handle_count: usize,
    pub handle_cap: usize,
    pub buffer: *mut u8,
    pub buffer_size: usize,
    pub buffer_cap: usize,
}

#[cfg(feature = "call")]
#[cfg(debug_assertions)]
pub fn test() {
    use core::ptr;

    use crate::*;

    fn rp(hdl: &mut [Handle], buf: &mut [u8]) -> RawPacket {
        RawPacket {
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
    crate::call::chan_new(&mut c1, &mut c2).expect("Failed to create a channel");
    let (c1, c2) = (c1, c2);

    // Test in 1 task (transfering to myself).
    let wo = {
        let wo = crate::call::wo_new().expect("Failed to create a wait object");
        crate::call::wo_notify(wo, 0).expect("Failed to notify the wait object");

        // Sending

        let mut buf = [1u8, 2, 3, 4, 5, 6, 7];
        let mut hdl = [wo];

        let sendee = rp(&mut hdl, &mut buf);
        crate::call::chan_send(c1, &sendee).expect("Failed to send a packet into the channel");

        {
            // Null handles can't be sent.
            hdl[0] = Handle::NULL;
            let mut sendee = rp(&mut hdl, &mut buf);
            let ret = crate::call::chan_send(c1, &sendee);
            assert_eq!(ret, Err(Error(EINVAL)));

            // The channel itself can't be sent.
            // To make connections to other tasks, use `init_channel`.
            hdl[0] = c1;
            sendee = rp(&mut hdl, &mut buf);
            let ret = crate::call::chan_send(c1, &sendee);
            assert_eq!(ret, Err(Error(EPERM)));

            // Neither can its peer.
            hdl[0] = c2;
            sendee = rp(&mut hdl, &mut buf);
            let ret = crate::call::chan_send(c1, &sendee);
            assert_eq!(ret, Err(Error(EPERM)));
        }

        {
            let mut receivee = rp(&mut [], &mut buf);
            let ret = crate::call::chan_recv(c2, &mut receivee, true);
            assert_eq!(ret, Err(Error(EBUFFER)));

            receivee = rp(&mut hdl, &mut []);
            let ret = crate::call::chan_recv(c2, &mut receivee, true);
            assert_eq!(ret, Err(Error(EBUFFER)));
        }

        buf.fill(0);
        let mut receivee = rp(&mut hdl, &mut buf);
        crate::call::chan_recv(c2, &mut receivee, true)
            .expect("Failed to receive a packet from the channel");
        assert_eq!(buf, [1u8, 2, 3, 4, 5, 6, 7]);

        let wo = hdl[0];
        crate::call::wo_notify(wo, 0).expect("Failed to notify the wait object");

        receivee = rp(&mut hdl, &mut buf);
        let ret = crate::call::chan_recv(c2, &mut receivee, false);
        assert_eq!(ret, Err(Error(ENOENT)));

        wo
    };
