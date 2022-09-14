use core::slice;

use sv_call::{
    ipc::{RawPacket, MAX_BUFFER_SIZE, MAX_HANDLE_COUNT},
    *,
};

use super::*;
use crate::{
    cpu::{arch::apic::TriggerMode, time},
    sched::{Dispatcher, WaiterData, SIG_READ},
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
    UserPtr::<In, u8>::new(packet.buffer).check_slice(packet.buffer_size)?;

    let handles = unsafe { slice::from_raw_parts(packet.handles, packet.handle_count) };
    if handles.contains(&hdl) {
        return Err(EPERM);
    }
    let buffer = unsafe { slice::from_raw_parts(packet.buffer, packet.buffer_size) };

    SCHED.with_current(|cur| {
        let map = cur.space().handles();
        let channel = map.get::<Channel>(hdl)?;
        if !channel.features().contains(Feature::WRITE) {
            return Err(EPERM);
        }
        let objects = unsafe { map.send(handles, &channel) }?;
        let mut packet = Packet::new(packet.id, objects, buffer);
        send(&channel, &mut packet)
    })
}

#[inline]
fn read_raw(packet_ptr: UserPtr<In, RawPacket>) -> Result<RawPacket> {
    let raw = unsafe { packet_ptr.read()? };
    UserPtr::<Out, Handle>::new(raw.handles).check_slice(raw.handle_cap)?;
    UserPtr::<Out, u8>::new(raw.buffer).check_slice(raw.buffer_cap)?;

    Ok(raw)
}

#[inline]
fn receive_handles<E: ?Sized + Event>(
    res: Result<Packet>,
    map: &crate::sched::task::hdl::HandleMap,
    raw: &mut RawPacket,
    event: &Arc<E>,
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
        receive_handles(res, map, &mut raw, (**channel).event())
    });

    write_raw_with_rest_of_packet(packet_ptr.out(), raw, res)
}

#[syscall]
fn chan_csend(hdl: Handle, packet: UserPtr<In, RawPacket>) -> Result<usize> {
    chan_send_impl(hdl, packet, |channel, packet| channel.call_send(packet))
}

#[syscall]
fn chan_crecv(
    hdl: Handle,
    id: usize,
    packet_ptr: UserPtr<InOut, RawPacket>,
    timeout_us: u64,
) -> Result {
    hdl.check_null()?;

    let mut raw = read_raw(packet_ptr.r#in())?;

    let call_event = SCHED.with_current(|cur| {
        let channel = cur.space().handles().get::<Channel>(hdl)?;
        if !{ channel.features() }.contains(Feature::WAIT | Feature::READ) {
            return Err(EPERM);
        }
        Ok(channel.call_event(id)? as _)
    })?;
    let blocker = if timeout_us == 0 {
        None
    } else {
        let pree = PREEMPT.lock();
        let blocker = crate::sched::Blocker::new(&call_event, true, SIG_READ);
        blocker.wait(Some(pree), time::from_us(timeout_us))?;
        Some(blocker)
    };

    let res = SCHED.with_current(|cur| {
        let map = cur.space().handles();

        let channel = map.get::<Channel>(hdl)?;
        if !channel.features().contains(Feature::READ) {
            return Err(EPERM);
        }

        raw.buffer_size = raw.buffer_cap;
        raw.handle_count = raw.handle_cap;
        let res = channel.call_receive(id, &mut raw.buffer_size, &mut raw.handle_count);
        receive_handles(res, map, &mut raw, &call_event)
    });

    if let Some(blocker) = blocker {
        if !blocker.detach().0 {
            return Err(ETIME);
        }
    }

    write_raw_with_rest_of_packet(packet_ptr.out(), raw, res)
}

#[syscall]
fn chan_acrecv(hdl: Handle, id: usize, wake_all: bool) -> Result<Handle> {
    SCHED.with_current(|cur| {
        let chan = cur.space().handles().get::<Channel>(hdl)?;
        if !{ chan.features() }.contains(Feature::READ | Feature::WAIT) {
            return Err(EPERM);
        }
        let event = chan.call_event(id)? as _;

        let blocker = crate::sched::Blocker::new(&event, wake_all, SIG_READ);
        cur.space().handles().insert(blocker, None)
    })
}

#[syscall]
fn chan_acrecv2(hdl: Handle, id: usize, disp: Handle) -> Result<usize> {
    hdl.check_null()?;
    disp.check_null()?;

    SCHED.with_current(|cur| {
        let chan = cur.space().handles().get::<Channel>(hdl)?;
        let disp = cur.space().handles().get::<Dispatcher>(disp)?;
        if !{ chan.features() }.contains(Feature::READ | Feature::WAIT) {
            return Err(EPERM);
        }
        if !disp.features().contains(Feature::WRITE) {
            return Err(EPERM);
        }
        let event = chan.call_event(id)? as _;

        let waiter_data = WaiterData::new(TriggerMode::Level, SIG_READ);
        Ok(disp.push(&event, waiter_data))
    })
}
