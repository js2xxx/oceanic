use core::slice;

use sv_call::{
    ipc::{RawPacket, MAX_BUFFER_SIZE, MAX_HANDLE_COUNT},
    *,
};

use super::*;
use crate::{
    sched::SIG_READ,
    syscall::{In, InOut, Out, UserPtr},
};

#[syscall]
fn chan_new(p1: UserPtr<Out, Handle>, p2: UserPtr<Out, Handle>) -> Result {
    p1.check()?;
    p2.check()?;
    SCHED.with_current(|cur| {
        let (c1, c2) = Channel::new();
        let map = cur.space().handles();
        let e1 = Arc::downgrade(&c1.me.event) as _;
        let e2 = Arc::downgrade(&c2.me.event) as _;
        let h1 = map.insert(c1, Some(e1))?;
        let h2 = map.insert(c2, Some(e2))?;
        unsafe {
            p1.write(h1)?;
            p2.write(h2)
        }
    })
}

fn chan_send_impl<F, R>(hdl: Handle, packet: UserPtr<In, RawPacket>, send: F) -> Result<R>
where
    F: FnOnce(&Channel, &mut Packet) -> Result<R>,
{
    hdl.check_null()?;

    let packet = unsafe { packet.read()? };
    if packet.buffer_size > MAX_BUFFER_SIZE || packet.handle_count >= MAX_HANDLE_COUNT {
        return Err(ENOMEM);
    }
    UserPtr::<In, Handle>::new(packet.handles).check_slice(packet.handle_count)?;
    UserPtr::<In>::new(packet.buffer).check_slice(packet.buffer_size)?;

    let handles = unsafe { slice::from_raw_parts(packet.handles, packet.handle_count) };
    if handles.contains(&hdl) {
        return Err(EPERM);
    }
    let buffer = unsafe { slice::from_raw_parts(packet.buffer, packet.buffer_size) };

    SCHED.with_current(|cur| {
        let map = cur.space().handles();
        let obj = map.get::<Channel>(hdl)?;
        if !obj.features().contains(Feature::WRITE) {
            return Err(EPERM);
        }
        let channel = Arc::clone(&obj);
        drop(obj);

        let objects = map.send(handles, &channel)?;
        let mut packet = Packet::new(packet.id, objects, buffer);
        send(&channel, &mut packet)
    })
}

#[inline]
fn read_raw(packet_ptr: UserPtr<In, RawPacket>) -> Result<RawPacket> {
    let raw = unsafe { packet_ptr.read()? };
    UserPtr::<Out, Handle>::new(raw.handles).check_slice(raw.handle_cap)?;
    UserPtr::<Out>::new(raw.buffer).check_slice(raw.buffer_cap)?;

    Ok(raw)
}

#[inline]
fn receive_handles<E: ?Sized + Event>(
    res: Result<Packet>,
    map: &crate::sched::task::hdl::HandleMap,
    raw: &mut RawPacket,
    event: Arc<E>,
) -> Result<Packet> {
    match res {
        Ok(mut packet) => {
            let handles = unsafe { slice::from_raw_parts_mut(raw.handles, raw.handle_cap) };
            map.receive(&mut packet.objects, handles);
            event.notify(SIG_READ, 0);
            Ok(packet)
        }
        Err(e) => Err(e),
    }
}

#[inline]
fn write_raw_with_rest_of_packet(
    packet_ptr: UserPtr<Out, RawPacket>,
    mut raw: RawPacket,
    res: Result<Packet>,
) -> Result {
    let ret = res.map(|packet| unsafe {
        raw.id = packet.id;
        raw.buffer
            .copy_from_nonoverlapping(packet.buffer().as_ptr(), packet.buffer().len());
    });

    unsafe { packet_ptr.write(raw) }?;
    ret
}

#[syscall]
fn chan_send(hdl: Handle, packet: UserPtr<In, RawPacket>) -> Result {
    chan_send_impl(hdl, packet, |channel, packet| channel.send(packet))
}

#[syscall]
fn chan_recv(hdl: Handle, packet_ptr: UserPtr<InOut, RawPacket>) -> Result {
    hdl.check_null()?;

    let mut raw = read_raw(packet_ptr.r#in())?;

    let res = SCHED.with_current(|cur| {
        let map = cur.space().handles();
        let channel = map.get::<Channel>(hdl)?;
        if !channel.features().contains(Feature::READ) {
            return Err(EPERM);
        }

        raw.buffer_size = raw.buffer_cap;
        raw.handle_count = raw.handle_cap;
        let res = channel.receive(&mut raw.buffer_size, &mut raw.handle_count);
        let event = (**channel).event().clone();
        drop(channel);
        receive_handles(res, map, &mut raw, event)
    });

    write_raw_with_rest_of_packet(packet_ptr.out(), raw, res)
}
