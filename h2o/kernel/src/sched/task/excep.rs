use alloc::vec;
use core::{
    mem::{self, MaybeUninit},
    slice,
    time::Duration,
};

use archop::reg::cr2;
use bytes::Buf;
use solvent::task::excep::{Exception, ExceptionResult, EXRES_CODE_OK};

use super::ctx::x86_64::Frame;
use crate::{
    cpu::intr::arch::ExVec,
    sched::{ipc::Packet, PREEMPT, SCHED},
};

pub fn dispatch_exception(frame: *mut Frame, vec: ExVec) -> bool {
    let slot = match SCHED.with_current(|cur| {
        unsafe { &*cur.tid.from.get() }
            .as_ref()
            .and_then(|from| from.1.as_ref())
            .map(|child| child.excep_chan())
    }) {
        Some(Some(slot)) => slot,
        _ => return false,
    };

    let excep_chan = match PREEMPT.scope(|| slot.lock().take()) {
        Some(chan) => chan,
        _ => return false,
    };

    let data: [u8; mem::size_of::<Exception>()] = unsafe {
        mem::transmute(Exception {
            vec: vec as u8,
            errc: (*frame).errc_vec,
            cr2: match vec {
                ExVec::PageFault => cr2::read(),
                _ => 0,
            },
        })
    };

    let excep = Packet::new(vec![], &data);
    if let Err(_) = excep_chan.send(excep) {
        PREEMPT.scope(|| *slot.lock() = Some(excep_chan));
        return false;
    }

    let ret = match excep_chan.receive(Duration::MAX) {
        Ok(mut res) => {
            let mut res = res.take().unwrap();
            let mut data = MaybeUninit::<ExceptionResult>::uninit();
            res.buffer_mut().copy_to_slice(unsafe {
                slice::from_raw_parts_mut(
                    data.as_mut_ptr().cast(),
                    mem::size_of::<ExceptionResult>(),
                )
            });

            let res = unsafe { data.assume_init() };
            Some(res.code == EXRES_CODE_OK)
        }
        Err(err) => match err {
            crate::sched::ipc::IpcError::ChannelClosed(_) => None,
            _ => Some(false),
        },
    };

    ret.map_or(false, |ret| {
        PREEMPT.scope(|| *slot.lock() = Some(excep_chan));
        ret
    })
}