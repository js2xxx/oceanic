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
use core::mem::MaybeUninit;

use bootfs::parse::Directory;
use solvent::prelude::*;
use svrt::{HandleInfo, HandleType, StartupArgs};
use targs::{HandleIndex, Targs};

extern crate alloc;

static mut ROOT_VIRT: MaybeUninit<Virt> = MaybeUninit::uninit();

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

fn sub_phys(bin_data: &[u8], bootfs: Directory, bootfs_phys: &Phys) -> Result<Phys> {
    let offset = offset_sub(bin_data, bootfs.image()).ok_or(Error::ERANGE)?;
    bootfs_phys.create_sub(offset, bin_data.len().next_multiple_of(PAGE_SIZE), false)
}

fn map_bootfs(phys: &Phys, root: &Virt) -> Directory<'static> {
    let ptr = root
        .map_phys(
            None,
            Phys::clone(phys),
            Flags::READABLE | Flags::EXECUTABLE | Flags::USER_ACCESS,
        )
        .expect("Failed to map boot FS");
    Directory::root(unsafe { ptr.as_ref() }).expect("Failed to parse boot filesystem")
}

#[no_mangle]
extern "C" fn tmain(init_chan: sv_call::Handle) {
    dbglog::init(log::Level::Debug);
    log::info!("Starting initialization");

    let init_chan = unsafe { Channel::from_raw(init_chan) };
    let mut buffer = [0; core::mem::size_of::<Targs>()];
    let mut handles = [MaybeUninit::uninit(); HandleIndex::Len as usize];
    init_chan
        .receive_raw(&mut buffer, &mut handles)
        .0
        .expect("Failed to receive the initial packet");

    let _targs = {
        let mut targs = Targs::default();
        plain::copy_from_bytes(&mut targs, &buffer).expect("Failed to get TINIT args");
        targs
    };

    let root_virt = unsafe {
        &*ROOT_VIRT.write(Virt::from_raw(
            handles[HandleIndex::RootVirt as usize].assume_init(),
        ))
    };

    mem::init();

    unsafe { test::test_syscall(root_virt) };

    let vdso_phys = unsafe { Phys::from_raw(handles[HandleIndex::Vdso as usize].assume_init()) };

    let bootfs_phys =
        unsafe { Phys::from_raw(handles[HandleIndex::Bootfs as usize].assume_init()) };
    let bootfs = map_bootfs(&bootfs_phys, root_virt);

    let bin = {
        let bin_data = bootfs
            .find(b"bin/test-bin", b'/')
            .expect("Failed to find test_bin");

        let bin_phys =
            sub_phys(bin_data, bootfs, &bootfs_phys).expect("Failed to create the physical object");
        unsafe { load::Image::new(bin_data, bin_phys) }.expect("Failed to create the image")
    };

    let (space, virt) = Space::new();
    let (entry, stack) =
        load::load_elf(bin.clone(), bootfs, &bootfs_phys, &virt).expect("Failed to load test_bin");

    let vdso_base = virt
        .map_vdso(Phys::clone(&vdso_phys))
        .expect("Failed to load VDSO");
    log::debug!("{:?} {:?} {:?}", entry, stack, vdso_base);

    let (me, child) = Channel::new();
    let child = child
        .reduce_features(Feature::SEND | Feature::READ)
        .expect("Failed to reduce features for read");
    let me = me
        .reduce_features(Feature::SEND | Feature::WRITE)
        .expect("Failed to reduce features for write");

    let startup_args = StartupArgs {
        handles: [
            (
                HandleInfo::new().with_handle_type(HandleType::RootVirt),
                Virt::into_raw(virt),
            ),
            (
                HandleInfo::new().with_handle_type(HandleType::VdsoPhys),
                Phys::into_raw(vdso_phys),
            ),
            (
                HandleInfo::new().with_handle_type(HandleType::ProgramPhys),
                Phys::into_raw(bin.phys),
            ),
        ]
        .into_iter()
        .collect(),
        args: vec![],
        envs: vec![],
    };

    me.send(startup_args).expect("Failed to send packet");

    let task = Task::exec(
        Some("PROGMGR"),
        space,
        entry,
        stack,
        Some(child),
        vdso_base.as_mut_ptr() as u64,
    )
    .expect("Failed to create the task");

    log::debug!("Waiting for the task");

    let retval = task.join().expect("Failed to join the task");
    log::debug!("{} {:?}", retval, Error::try_from_retval(retval));

    log::debug!("Reaching end of TINIT");
}
