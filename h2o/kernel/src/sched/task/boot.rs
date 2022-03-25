use core::mem;

use archop::Azy;
use bitop_ex::BitOpEx;
use sv_call::Feature;
use targs::Targs;

use super::*;
use crate::{
    mem::space::{Flags, Phys},
    sched::SCHED,
};

static VDSO_DATA: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/vdso"));
pub static VDSO: Azy<(Flags, Phys)> = Azy::new(|| {
    let flags = Flags::READABLE | Flags::EXECUTABLE | Flags::USER_ACCESS;
    let vdso_mem = Phys::allocate(VDSO_DATA.len().round_up_bit(paging::PAGE_SHIFT), false)
        .expect("Failed to allocate memory for VDSO");
    unsafe {
        let dst = vdso_mem.base().to_laddr(minfo::ID_OFFSET);
        dst.copy_from_nonoverlapping(VDSO_DATA.as_ptr(), VDSO_DATA.len());
    }
    (flags, vdso_mem)
});
pub static BOOTFS: Azy<(Flags, Phys)> = Azy::new(|| {
    (
        Flags::READABLE | Flags::EXECUTABLE | Flags::USER_ACCESS,
        Phys::new(
            crate::kargs().bootfs_phys,
            crate::kargs().bootfs_len.round_up_bit(paging::PAGE_SHIFT),
        )
        .expect("Failed to create boot FS object"),
    )
});

fn flags_to_feat(flags: Flags) -> Feature {
    let mut feat = Feature::SEND | Feature::SYNC;
    if flags.contains(Flags::READABLE) {
        feat |= Feature::READ
    }
    if flags.contains(Flags::WRITABLE) {
        feat |= Feature::WRITE
    }
    if flags.contains(Flags::EXECUTABLE) {
        feat |= Feature::EXECUTE
    }
    feat
}

pub fn setup() {
    let mut objects = hdl::List::new();

    // The sequence of kernel objects must match the one defined in
    // `targs::HandleIndex`.
    {
        let mem_res = Arc::clone(crate::dev::mem_resource());
        let res = unsafe {
            objects.insert_impl(
                hdl::Ref::try_new(mem_res, None)
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
                hdl::Ref::try_new(pio_res, None)
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
                hdl::Ref::try_new(gsi_res, None)
                    .expect("Failed to create GSI resource")
                    .coerce_unchecked(),
            )
        };
        res.expect("Failed to insert GSI resource");
    }
    unsafe {
        objects
            .insert_impl(
                hdl::Ref::try_new_unchecked(Phys::clone(&VDSO.1), flags_to_feat(VDSO.0), None)
                    .expect("Failed to create VDSO reference")
                    .coerce_unchecked(),
            )
            .expect("Failed to insert VDSO");

        objects
            .insert_impl(
                hdl::Ref::try_new_unchecked(Phys::clone(&BOOTFS.1), flags_to_feat(BOOTFS.0), None)
                    .expect("Failed to create boot FS reference")
                    .coerce_unchecked(),
            )
            .expect("Failed to insert boot FS");
    }

    let buf = {
        let targs = Targs {
            rsdp: *crate::kargs().rsdp,
            smbios: *crate::kargs().smbios,
            bootfs_size: crate::kargs().bootfs_len,
        };
        unsafe { mem::transmute::<_, [u8; mem::size_of::<Targs>()]>(targs) }
    };

    let (me, chan) = Channel::new();
    let event = Arc::downgrade(chan.event()) as _;
    let chan = unsafe {
        hdl::Ref::try_new(chan, Some(event))
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
