#![no_std]
#![no_main]
#![allow(unused_unsafe)]
#![feature(alloc_error_handler)]
#![feature(alloc_layout_extra)]
#![feature(box_syntax)]
#![feature(int_roundings)]
#![feature(lang_items)]
#![feature(min_specialization)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(result_option_inspect)]
#![feature(slice_ptr_get)]
#![feature(slice_ptr_len)]
#![feature(thread_local)]
#![feature(toowned_clone_into)]
#![feature(vec_into_raw_parts)]

mod load;
mod mem;
mod rxx;
mod test;

use alloc::vec;

use bootfs::parse::Directory;
use solvent::prelude::*;
use targs::{HandleIndex, Targs};

extern crate alloc;

fn is_sub<T>(slice: &[T], parent: &[T]) -> bool {
    let range = parent.as_ptr_range();
    let srange = slice.as_ptr_range();
    range.start <= srange.start && srange.end <= range.end
}

fn offset_sub<T>(slice: &[T], parent: &[T]) -> Option<usize> {
    is_sub(slice, parent).then(|| {
        let start = parent.as_ptr();
        let end = slice.as_ptr();
        // SAFETY: `slice` is part of `parent`
        unsafe { end.offset_from(start) as usize }
    })
}

fn sub_phys(bin_data: &[u8], bootfs: Directory, bootfs_phys: &PhysRef) -> Result<PhysRef> {
    let offset = offset_sub(bin_data, bootfs.image()).ok_or(Error::ERANGE)?;
    bootfs_phys
        .dup_sub(offset, bin_data.len())
        .ok_or(Error::ERANGE)
}

fn map_bootfs(phys: &PhysRef) -> Directory<'static> {
    let ptr = Space::current()
        .map_ref(
            None,
            PhysRef::clone(phys),
            Flags::READABLE | Flags::EXECUTABLE | Flags::USER_ACCESS,
        )
        .expect("Failed to map boot FS");
    Directory::root(unsafe { ptr.as_ref() }).expect("Failed to parse boot filesystem")
}

#[no_mangle]
extern "C" fn tmain(init_chan: sv_call::Handle) {
    dbglog::init(log::Level::Debug);
    log::info!("Starting initialization");
    mem::init();

    unsafe { test::test_syscall() };

    let init_chan = unsafe { Channel::from_raw(init_chan) };
    let mut packet = Default::default();
    { init_chan.receive(&mut packet) }.expect("Failed to receive the initial packet");

    let targs = {
        let mut targs = Targs::default();
        plain::copy_from_bytes(&mut targs, &packet.buffer).expect("Failed to get TINIT args");
        targs
    };

    let vdso_phys = unsafe { Phys::from_raw(packet.handles[HandleIndex::Vdso as usize]) };

    let bootfs_phys = unsafe { Phys::from_raw(packet.handles[HandleIndex::Bootfs as usize]) }
        .into_ref(targs.bootfs_size);
    let bootfs = map_bootfs(&bootfs_phys);

    let bin = {
        let bin_data = bootfs
            .find(b"bin/test-bin", b'/')
            .expect("Failed to find test_bin");

        let bin_phys =
            sub_phys(bin_data, bootfs, &bootfs_phys).expect("Failed to create the physical object");
        unsafe { load::Image::new(bin_data, bin_phys) }.expect("Failed to create the image")
    };

    let space = Space::new();
    let (entry, stack) =
        load::load_elf(bin, bootfs, &bootfs_phys, &space).expect("Failed to load test_bin");

    let vdso_base = space
        .map_vdso(Phys::clone(&vdso_phys))
        .expect("Failed to load VDSO");
    log::debug!("{:?} {:?}", entry, stack);

    let (me, child) = Channel::new();

    me.send(&Packet {
        buffer: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        handles: vec![],
        ..Default::default()
    })
    .expect("Failed to send packet");

    let task = Task::exec(
        Some("PROGMGR"),
        space,
        entry,
        stack,
        Some(child),
        vdso_base.as_ptr() as u64,
    )
    .expect("Failed to create the task");

    log::debug!("Waiting for the task");

    let retval = task.join().expect("Failed to join the task");
    log::debug!("{} {:?}", retval, Error::try_from_retval(retval));

    log::debug!("Reaching end of TINIT");
}
