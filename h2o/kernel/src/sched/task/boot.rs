use alloc::sync::Weak;
use core::mem;

use archop::Azy;
use bitop_ex::BitOpEx;
use sv_call::Feature;
use targs::Targs;

use super::{hdl::DefaultFeature, *};
use crate::{
    cpu::arch::tsc::TSC_CLOCK,
    mem::space::{Flags, Phys, Virt},
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
    unsafe {
        let constants = sv_call::Constants {
            ticks_offset: TSC_CLOCK.initial,
            ticks_multiplier: TSC_CLOCK.mul,
            ticks_shift: TSC_CLOCK.sft,
        };

        #[allow(clippy::zero_prefixed_literal)]
        let offset = include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/constant_offset.rs"
        ));
        let ptr = { VDSO.1.base().to_laddr(minfo::ID_OFFSET) }
            .add(offset)
            .cast::<sv_call::Constants>();
        ptr.write(constants);
    }

    let mut objects = hdl::List::new();

    // The sequence of kernel objects must match the one defined in
    // `targs::HandleIndex`.
    {
        let mem_res = Arc::clone(crate::dev::mem_resource());
        let res = objects
            .insert(hdl::Ref::try_new(mem_res, None).expect("Failed to create memory resource"));
        res.expect("Failed to insert memory resource");
    }
    {
        let pio_res = Arc::clone(crate::dev::pio_resource());
        let res = objects
            .insert(hdl::Ref::try_new(pio_res, None).expect("Failed to create port I/O resource"));
        res.expect("Failed to insert port I/O resource");
    }
    {
        let gsi_res = Arc::clone(crate::dev::gsi_resource());
        let res = objects
            .insert(hdl::Ref::try_new(gsi_res, None).expect("Failed to create GSI resource"));
        res.expect("Failed to insert GSI resource");
    }
    unsafe {
        let res = objects.insert(
            hdl::Ref::try_new_unchecked(Phys::clone(&VDSO.1), flags_to_feat(VDSO.0), None)
                .expect("Failed to create VDSO reference"),
        );
        res.expect("Failed to insert VDSO");

        let res = objects.insert(
            hdl::Ref::try_new_unchecked(Phys::clone(&BOOTFS.1), flags_to_feat(BOOTFS.0), None)
                .expect("Failed to create boot FS reference"),
        );
        res.expect("Failed to insert boot FS");
    }
    let space = super::Space::new(Type::User).expect("Failed to create space");
    unsafe {
        let res = objects.insert(
            hdl::Ref::try_new_unchecked(
                Arc::downgrade(space.mem().root()),
                Weak::<Virt>::default_features() | Feature::SEND,
                None,
            )
            .expect("Failed to create root virt"),
        );
        res.expect("Failed to insert root virt");
    }

    let buf = {
        let targs = Targs {
            rsdp: *crate::kargs().rsdp,
            smbios: *crate::kargs().smbios,
        };
        unsafe { mem::transmute::<_, [u8; mem::size_of::<Targs>()]>(targs) }
    };

    let (me, chan) = Channel::new();
    let event = Arc::downgrade(chan.event()) as _;
    let chan = unsafe { hdl::Ref::try_new(chan, Some(event)).expect("Failed to create channel") };
    me.send(&mut crate::sched::ipc::Packet::new(0, objects, &buf))
        .expect("Failed to send message");
    let image = unsafe {
        core::slice::from_raw_parts(
            *crate::kargs().tinit_phys.to_laddr(minfo::ID_OFFSET),
            crate::kargs().tinit_len,
        )
    };
    let tinit = from_elf(
        image,
        space,
        String::from("TINIT"),
        crate::cpu::all_mask(),
        chan,
    )
    .expect("Failed to initialize TINIT");
    SCHED.unblock(tinit, true);

    // Get rid of EPIPE in TINIT.
    mem::forget(me);
}
