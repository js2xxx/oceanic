#![no_std]
#![no_main]
#![feature(slice_ptr_get)]

use solvent::prelude::{Object, Phys};
use svrt::HandleType;

extern crate alloc;

async fn main() {
    unsafe { dldisconn() };
    log::debug!("Hello world!");
    solvent_std::env::args().for_each(|arg| log::debug!("{arg}"));

    solvent_async::test::test_disp().await;

    let bootfs_phys = svrt::take_startup_handle(HandleType::BootfsPhys.into());
    let bootfs_phys = unsafe { Phys::from_raw(bootfs_phys) };
    let _bootfs = boot::Dir::build(&bootfs_phys);

    log::debug!("Goodbye!");
}

solvent_async::entry!(main, solvent_std);

#[link(name = "ldso")]
extern "C" {
    fn dldisconn();
}

mod boot {
    use alloc::{borrow::ToOwned, collections::BTreeMap, ffi::CString};
    use core::{ffi::CStr, ptr::NonNull};

    use either::Either;
    use solvent::prelude::{Flags, Phys, PAGE_MASK};
    use solvent_std::sync::Arsc;

    pub struct Dir {
        entries: BTreeMap<CString, Arsc<Node>>,
    }

    #[allow(dead_code)]
    struct File {
        data: Phys,
        len: usize,
    }

    enum Node {
        Dir(Dir),
        File(File),
    }

    impl Dir {
        /// # Safety
        ///
        /// The caller must ensure `dir` is the sub slice of `root_virt` and
        /// `root_phys` is mapped into `root_virt` with offset 0, full length
        /// and readable & executable flags.
        unsafe fn insert(
            &mut self,
            root_phys: &Phys,
            base: NonNull<u8>,
            dir: bootfs::parse::Directory,
        ) {
            for dir_entry in dir.iter() {
                let metadata = dir_entry.metadata();
                assert!(metadata.version == bootfs::VERSION);
                let name = unsafe { CStr::from_ptr(metadata.name.as_ptr() as _) }.to_owned();
                let node = match dir_entry.content() {
                    Either::Right(dir_slice) => {
                        let mut dir = Dir {
                            entries: BTreeMap::new(),
                        };
                        dir.insert(root_phys, base, dir_slice);
                        Node::Dir(dir)
                    }
                    Either::Left(data) => {
                        let offset = unsafe { data.as_ptr().offset_from(base.as_ptr()) as usize };
                        assert!(
                            offset & PAGE_MASK == 0,
                            "offset is not aligned: {:#x}",
                            offset
                        );
                        let len = data.len();
                        let data = root_phys
                            .create_sub(offset, (len + PAGE_MASK) & !PAGE_MASK, false)
                            .expect("Failed to create sub phys");
                        let file = File { data, len };
                        Node::File(file)
                    }
                };
                self.entries.insert(name, Arsc::new(node));
            }
        }

        pub fn build(root_phys: &Phys) -> Dir {
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
                let mut dir = Dir {
                    entries: BTreeMap::new(),
                };
                dir.insert(root_phys, base.as_non_null_ptr(), root);

                // We only use image before unmapping.
                svrt::root_virt()
                    .unmap(base.as_non_null_ptr(), base.len(), true)
                    .expect("Failed to unmap the root phys");
                dir
            }
        }
    }
}
