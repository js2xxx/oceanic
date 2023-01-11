use alloc::{borrow::ToOwned, vec::Vec};
use core::{ffi::CStr, ptr::NonNull};

use either::Either;
use solvent::prelude::{Flags, Object, Phys, PAGE_MASK};
use solvent_fs::{
    entry::Entry,
    fs,
    mem::{
        dir::{RecursiveBuild, RecursiveBuilder},
        file::MemFile,
    },
};
use solvent_rpc::{
    io::{dir::Directory, OpenOptions, Permission},
    Protocol,
};
use solvent_std::{
    path::Path,
    sync::{Arsc, Once},
};
use svrt::HandleType;

/// # Safety
///
/// The caller must ensure `dir` is the sub slice of `root_virt` and
/// `root_phys` is mapped into `root_virt` with offset 0, full length
/// and readable & executable flags.
unsafe fn build_inner(
    root_phys: &Phys,
    base: NonNull<u8>,
    dir: bootfs::parse::Directory,
) -> Vec<RecursiveBuild> {
    let mut ret = Vec::new();
    for dir_entry in dir.iter() {
        let metadata = dir_entry.metadata();
        assert!(metadata.version == bootfs::VERSION);

        let name = unsafe { CStr::from_ptr(metadata.name.as_ptr() as _) }
            .to_str()
            .unwrap()
            .to_owned();

        match dir_entry.content() {
            Either::Right(dir_slice) => {
                ret.push(RecursiveBuild::Down(
                    name,
                    Permission::READ | Permission::EXECUTE,
                ));
                ret.append(&mut build_inner(root_phys, base, dir_slice));
                ret.push(RecursiveBuild::Up);
            }
            Either::Left(data) => {
                let offset = unsafe { data.as_ptr().offset_from(base.as_ptr()) as usize };
                assert!(
                    offset & PAGE_MASK == 0,
                    "offset is not aligned: {offset:#x}"
                );
                let len = data.len();
                let data = root_phys
                    .create_sub(offset, (len + PAGE_MASK) & !PAGE_MASK, false)
                    .expect("Failed to create sub phys");
                let file = MemFile::new(data, Permission::READ | Permission::EXECUTE);
                ret.push(RecursiveBuild::Entry(name, Arsc::new(file)));
            }
        };
    }
    ret
}

fn builder(root_phys: &Phys) -> Vec<RecursiveBuild> {
    let base = svrt::root_virt()
        .map_phys(
            None,
            root_phys.clone(),
            Flags::READABLE | Flags::EXECUTABLE | Flags::USER_ACCESS,
        )
        .expect("Failed to map root phys");

    unsafe {
        let image = base.as_ref();
        let root = bootfs::parse::Directory::root(image).expect("Failed to parse root dir");
        let builder = build_inner(root_phys, base.as_non_null_ptr(), root);

        // We only use image before unmapping.
        svrt::root_virt()
            .unmap(base.as_non_null_ptr(), base.len(), true)
            .expect("Failed to unmap the root phys");
        builder
    }
}

pub fn mount() {
    static MOUNT: Once = Once::new();
    MOUNT.call_once(|| {
        let bootfs_phys = svrt::take_startup_handle(HandleType::BootfsPhys.into());
        let bootfs_phys = unsafe { Phys::from_raw(bootfs_phys) };
        let bootfs = builder(&bootfs_phys)
            .into_iter()
            .build(Permission::READ | Permission::EXECUTE)
            .expect("Failed to build the bootfs dir");

        let (client, server) = Directory::sync_channel();
        bootfs
            .open(
                solvent_fs::spawner(),
                Default::default(),
                Path::new(""),
                OpenOptions::READ | OpenOptions::EXECUTE,
                server.try_into().unwrap(),
            )
            .expect("Failed to open a connection");
        fs::local()
            .mount("boot", client.into())
            .expect("Failed to mount to vfs");
    })
}
