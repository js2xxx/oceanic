//! The FS module of H2O's boot loader.

pub mod elf;
pub mod tar;

use uefi::{
    prelude::*,
    proto::{
        loaded_image::*,
        media::{file::File, *},
    },
};

/// The volume where the boot loader and other files are located.
static mut LOCAL_VOL: Option<file::Directory> = None;

/// Initialize the FS module.
pub fn init(img: Handle, syst: &SystemTable<Boot>) {
    log::trace!("file::init: syst = {:?}", syst as *const _);

    let bs = syst.boot_services();

    // Get the boot loader image.
    let local_img = bs
        .handle_protocol::<LoadedImage>(img)
        .expect_success("Failed to locate loaded image protocol");

    // Get the file system of the device where the BL image is located.
    let fs = bs
        .handle_protocol::<fs::SimpleFileSystem>(unsafe { &*local_img.get() }.device())
        .expect_success("Failed to locate file system protocol");

    unsafe {
        // Open the volume with the file system.
        LOCAL_VOL = Some(
            (&mut *fs.get())
                .open_volume()
                .expect_success("Failed to open the local volume"),
        );
    }
}

/// Load a file in the local volume.
///
/// # Returns
///
/// This function returns a tuple with 2 elements where the former element is
/// the physical address of the loaded file, and the latter is the file size. It
/// can be used to construct a slice to read data.
pub fn load(syst: &SystemTable<Boot>, filename: &str) -> *mut [u8] {
    log::trace!("file::load: filename = {}", filename);

    // Take the volume out of the local static variable to keep consistency.
    let mut volume = unsafe {
        LOCAL_VOL
            .take()
            .expect("The local volume should be initialized")
    };

    let mut kfile = volume
        .open(filename, file::FileMode::Read, file::FileAttribute::empty())
        .expect_success("Failed to open kernel file");

    let ksize = {
        let mut finfo_buffer = alloc::vec![0; paging::PAGE_SIZE];
        let finfo: &mut file::FileInfo = kfile
            .get_info(&mut finfo_buffer)
            .expect_success("Failed to get kernel file information");

        finfo.file_size() as usize
    };

    // We need to manually allocate the memory for the kernel file instead of
    // creating a new `Vec<u8>` because we need to align the file properly and
    // the latter is badly aligned.
    let (kfile_addr, kfile_data) = crate::mem::alloc(syst)
        .alloc_into_slice(ksize, crate::mem::EFI_ID_OFFSET)
        .expect("Failed to allocate memory for the kernel file");

    match kfile
        .into_type()
        .expect_success("Failed to deduce kernel file type")
    {
        file::FileType::Regular(mut kfile) => {
            let asize = kfile
                .read(unsafe { &mut *kfile_data })
                .expect_success("Failed to read kernel file");
            assert!(
                asize == ksize,
                "Failed to read whole kernel file: read {:#x}, required {:#x}",
                asize,
                ksize
            );
        }
        _ => panic!("Kernel file should be a regular file"),
    }

    // Put back the local volume.
    unsafe { LOCAL_VOL = Some(volume) };

    let ptr = *kfile_addr.to_laddr(crate::mem::EFI_ID_OFFSET);
    unsafe { core::slice::from_raw_parts_mut(ptr, ksize) }
}

pub fn realloc_file(syst: &SystemTable<Boot>, data: &[u8]) -> *mut [u8] {
    let (_, dest_ptr) = crate::mem::alloc(&syst)
        .alloc_into_slice(data.len(), crate::mem::EFI_ID_OFFSET)
        .expect("Failed to allocate memory");
    let dest = unsafe { &mut *dest_ptr };
    dest.copy_from_slice(data);
    dest
}
