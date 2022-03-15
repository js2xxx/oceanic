use core::alloc::Layout;

use archop::Azy;
use bytes::{BufMut, BytesMut};

use super::*;
use crate::{
    mem::space::{Flags, Phys},
    sched::SCHED,
};

static VDSO_DATA: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/vdso"));
pub static VDSO: Azy<Arsc<Phys>> = Azy::new(|| {
    let vdso_layout = Layout::from_size_align(VDSO_DATA.len(), paging::PAGE_LAYOUT.align())
        .expect("Failed to get the layout of VDSO");
    let vdso_mem = Phys::allocate(vdso_layout, Flags::READABLE | Flags::EXECUTABLE)
        .expect("Failed to allocate memory for VDSO");
    unsafe {
        let dst = vdso_mem.base().to_laddr(minfo::ID_OFFSET);
        dst.copy_from_nonoverlapping(VDSO_DATA.as_ptr(), VDSO_DATA.len());
    }
    vdso_mem
});
pub static BOOTFS: Azy<Arsc<Phys>> = Azy::new(|| {
    let layout = Layout::from_size_align(crate::kargs().bootfs_len, paging::PAGE_LAYOUT.align())
        .expect("Failed to get the layout of boot FS");
    Phys::new(
        crate::kargs().bootfs_phys,
        layout,
        Flags::READABLE | Flags::EXECUTABLE,
    )
    .expect("Failed to create boot FS object")
});

pub fn setup() {
    let mut objects = hdl::List::new();
    let noevent = alloc::sync::Weak::<crate::sched::BasicEvent>::new();
    {
        let mem_res = Arc::clone(crate::dev::mem_resource());
        let res = unsafe {
            objects.insert_impl(
                hdl::Ref::try_new(mem_res, noevent.clone())
                    .expect("Failed to create memory resource")
                    .coerce_unchecked(),
            )
        };
        res.expect("Failed to insert memory resource");
    }
    {
        let pio_res = Arc::clone(crate::dev::pio_resource());
        let res = unsafe {
            objects.insert_impl(
                hdl::Ref::try_new(pio_res, noevent.clone())
                    .expect("Failed to create port I/O resource")
                    .coerce_unchecked(),
            )
        };
        res.expect("Failed to insert port I/O resource");
    }
    {
        let gsi_res = Arc::clone(crate::dev::gsi_resource());
        let res = unsafe {
            objects.insert_impl(
                hdl::Ref::try_new(gsi_res, noevent.clone())
                    .expect("Failed to create GSI resource")
                    .coerce_unchecked(),
            )
        };
        res.expect("Failed to insert GSI resource");
    }
    unsafe {
        objects
            .insert_impl(
                hdl::Ref::try_new(Arsc::clone(&VDSO), noevent.clone())
                    .expect("Failed to create VDSO reference")
                    .coerce_unchecked(),
            )
            .expect("Failed to insert VDSO");

        objects
            .insert_impl(
                hdl::Ref::try_new(Arsc::clone(&BOOTFS), noevent)
                    .expect("Failed to create boot FS reference")
                    .coerce_unchecked(),
            )
            .expect("Failed to insert boot FS");
    }

    let buf = {
        let mut buf = BytesMut::new();
        buf.put_u64(*crate::kargs().rsdp as u64);
        buf.put_u64(*crate::kargs().smbios as u64);
        buf
    };

    let (me, chan) = Channel::new();
    let event = Arc::downgrade(chan.event()) as _;
    let chan = unsafe {
        hdl::Ref::try_new(chan, event)
            .expect("Failed to create channel")
            .coerce_unchecked()
    };
    me.send(&mut crate::sched::ipc::Packet::new(0, objects, &buf))
        .expect("Failed to send message");
    let image = unsafe {
        core::slice::from_raw_parts(
            *crate::kargs().tinit_phys.to_laddr(minfo::ID_OFFSET),
            crate::kargs().tinit_len,
        )
    };
    let tinit = from_elf(image, String::from("TINIT"), crate::cpu::all_mask(), chan)
        .expect("Failed to initialize TINIT");
    SCHED.unblock(tinit, true);
}
