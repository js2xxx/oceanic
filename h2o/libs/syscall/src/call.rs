pub mod hdl;
#[cfg(feature = "call")]
pub mod raw;
pub mod reg;

use solvent_gen::syscall_stub;

#[allow(unused_imports)]
use crate::{ipc::RawPacket, Arguments, Handle, SerdeReg};

syscall_stub!(0 => pub(crate) fn get_time(ptr: *mut u128));
// #[cfg(debug_assertions)]
syscall_stub!(1 => pub(crate) fn log(args: *const ::log::Record));

syscall_stub!(2 => pub(crate) fn task_exit(retval: usize));
syscall_stub!(3 => pub(crate) fn task_exec(ci: *const crate::task::ExecInfo) -> Handle);
syscall_stub!(4 =>
    pub(crate) fn task_new(
        name: *const u8,
        name_len: usize,
        space: Handle,
        init: *mut Handle
    ) -> Handle
);
syscall_stub!(5 => pub(crate) fn task_join(hdl: Handle) -> usize);
syscall_stub!(6 => pub(crate) fn task_ctl(hdl: Handle, op: u32, data: *mut Handle));
syscall_stub!(7 =>
    pub(crate) fn task_debug(
        hdl: Handle,
        op: u32,
        addr: usize,
        data: *mut u8,
        len: usize
    )
);
syscall_stub!(8 => pub(crate) fn task_sleep(ms: u32));

syscall_stub!(9 => pub(crate) fn phys_alloc(size: usize, align: usize, flags: u32) -> Handle);
syscall_stub!(10 => pub(crate) fn mem_new() -> Handle);
syscall_stub!(11 => pub(crate) fn mem_map(space: Handle, mi: *const crate::mem::MapInfo) -> *mut u8);
syscall_stub!(12 => pub(crate) fn mem_reprot(space: Handle, ptr: *mut u8, len: usize, flags: u32));
syscall_stub!(13 => pub(crate) fn mem_unmap(space: Handle, ptr: *mut u8));

syscall_stub!(16 =>
    pub(crate) fn futex_wait(
        ptr: *mut u64,
        expected: u64,
        timeout_us: u64
    ) -> bool
);
syscall_stub!(17 => pub(crate) fn futex_wake(ptr: *mut u64, num: usize) -> usize);
syscall_stub!(18 =>
    pub(crate) fn futex_reque(
        ptr: *mut u64,
        wake_num: *mut usize,
        other: *mut u64,
        requeue_num: *mut usize,
    )
);

syscall_stub!(19 => pub(crate) fn obj_clone(hdl: Handle) -> Handle);
syscall_stub!(20 => pub(crate) fn obj_drop(hdl: Handle));

syscall_stub!(23 => pub(crate) fn chan_new(p1: *mut Handle, p2: *mut Handle));
syscall_stub!(24 => pub(crate) fn chan_send(hdl: Handle, packet: *const RawPacket));
syscall_stub!(25 => pub fn chan_recv(hdl: Handle, packet: *mut RawPacket, timeout_us: u64));
syscall_stub!(26 => pub(crate) fn chan_csend(hdl: Handle, packet: *const RawPacket) -> usize);
syscall_stub!(27 =>
    pub fn chan_crecv(
        hdl: Handle,
        id: usize,
        packet: *mut RawPacket,
        timeout_us: u64
    )
);

syscall_stub!(29 => pub(crate) fn event_new(wake_all: bool) -> Handle);
syscall_stub!(30 => pub(crate) fn event_wait(hdl: Handle, signal: u8, timeout_us: u64));
syscall_stub!(31 => pub(crate) fn event_notify(hdl: Handle, active: u8) -> usize);
syscall_stub!(32 => pub(crate) fn event_endn(hdl: Handle, masked: u8));
