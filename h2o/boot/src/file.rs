//! The FS module of H2O's boot loader.

use core::mem::size_of;
use elf_rs::*;
use uefi::prelude::*;
use uefi::proto::loaded_image::*;
use uefi::proto::media::{file::File, *};

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
            LOCAL_VOL = Some((&mut *fs.get())
                  .open_volume()
                  .expect_success("Failed to open the local volume"));
      }
}

/// Load a file in the local volume.
///
/// # Returns
///
/// This function returns a tuple with 2 elements where the former element is the physical address
/// of the loaded file, and the latter is the file size. It can be used to construct a slice to read
/// data.
pub fn load(syst: &SystemTable<Boot>, filename: &str) -> (paging::PAddr, usize) {
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

      let kfile_addr = {
            // We need to manually allocate the memory for the kernel file instead of creating a new
            // `Vec<u8>` because we need to align the file properly and the latter is badly aligned.
            let ksize_aligned = super::round_up_p2(ksize, paging::PAGE_SIZE);
            crate::mem::alloc(syst)
                  .alloc_n(ksize_aligned >> paging::PAGE_SHIFT)
                  .expect("Failed to allocate memory")
      };
      let mut kfile_data =
            unsafe { core::slice::from_raw_parts_mut(*kfile_addr as *mut u8, ksize) };

      match kfile
            .into_type()
            .expect_success("Failed to deduce kernel file type")
      {
            file::FileType::Regular(mut kfile) => {
                  let asize = kfile
                        .read(&mut kfile_data)
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
      (kfile_addr, ksize)
}

/// Transform the flags of a ELF program header into the attribute of a paging entry.
///
/// In this case, we only focus on the read/write-ability and executability.
fn flags_to_pg_attr(flags: u32) -> paging::Attr {
      const PF_W: u32 = 0x2;
      const PF_X: u32 = 0x1;

      let mut ret = paging::Attr::PRESENT;
      if (flags & PF_W) != 0 {
            ret |= paging::Attr::WRITABLE;
      }
      if (flags & PF_X) == 0 {
            ret |= paging::Attr::EXE_DISABLE;
      }
      ret
}

/// Load a loadable ELF program header.
///
/// The program segment mapping is like the graph below:
///
///       |<----File size: Directly mapping----->|<-Extra: Allocation & Mapping->|
///       |<----------------------Memory size----------------------------------->|
///
/// # Arguments
/// * `virt` - The base linear address where the segment should be loaded.
/// * `phys` - The base physical address where the segment is located.
/// * `fsize` - The size of the file stored in the media.
/// * `msize` - The size of the program required in the memory.
fn load_prog(
      syst: &SystemTable<Boot>,
      flags: u32,
      virt: paging::LAddr,
      phys: paging::PAddr,
      fsize: usize,
      msize: usize,
) {
      log::trace!(
            "file::load_prog: flags = {:?}, virt = {:?}, phys = {:?}, fsize = {:?}, msize = {:?}",
            flags,
            virt,
            phys,
            fsize,
            msize
      );

      let pg_attr = flags_to_pg_attr(flags);
      let (vstart, vend) = (virt.val(), virt.val() + fsize);

      if fsize > 0 {
            let virt = paging::LAddr::from(vstart)..paging::LAddr::from(vend);
            crate::mem::maps(syst, virt, phys, pg_attr).expect("Failed to map virtual memory");
      }

      if msize > fsize {
            let extra = msize - fsize;
            let phys = crate::mem::alloc(syst)
                  .alloc_n(extra >> paging::PAGE_SHIFT)
                  .expect("Failed to allocate extra memory");
            let virt = paging::LAddr::from(vend)..paging::LAddr::from(vend + extra);
            crate::mem::maps(syst, virt, phys, pg_attr).expect("Failed to map virtual memory");
      }
}

/// Load a Thread-Local Storage (TLS) segment.
fn load_tls(size: usize) {
      log::trace!("file::map: loading TLS: size = {:?}", size);

      unsafe {
            let tls_vec = alloc::vec::Vec::<u8>::with_capacity(size + size_of::<*mut usize>());
            let (tls, _, _) = tls_vec.into_raw_parts();
            let self_ptr = tls.add(size).cast::<usize>();
            // TLS's self-pointer is written its physical address there,
            // and therefore should be modified in the kernel.
            self_ptr.write(self_ptr as usize);

            const FS_BASE: u64 = 0xC0000100;
            asm!(
                  "wrmsr",
                  in("ecx") FS_BASE,
                  in("eax") self_ptr,
                  in("edx") self_ptr as u64 >> 32
            );
      };
}

/// Map a ELF executable into the memory.
///
/// # Returns
/// 
/// This function returns a tuple with 2 elements where the first element is the entry point of the 
/// ELF executable and the second element is the TLS size of it.
pub fn map_elf(syst: &SystemTable<Boot>, data: &[u8]) -> (*mut u8, Option<usize>) {
      log::trace!(
            "file::map: syst = {:?}, data = {:?}",
            syst as *const _,
            data.as_ptr()
      );

      let elf = Elf::from_bytes(data).expect("Failed to map ELF file");
      let elf = match elf {
            Elf::Elf64(e) => e,
            _ => panic!("ELF64 file accepted only"),
      };

      let mut tls_size = None;
      for phdr in elf.program_headers() {
            match phdr.ph_type() {
                  ProgramType::LOAD => load_prog(
                        syst,
                        phdr.flags(),
                        paging::LAddr::from(phdr.vaddr() as usize),
                        paging::PAddr::new(
                              unsafe { data.as_ptr().add(phdr.offset() as usize) } as usize
                        ),
                        super::round_up_p2(phdr.filesz() as usize, paging::PAGE_SIZE),
                        super::round_up_p2(phdr.memsz() as usize, paging::PAGE_SIZE),
                  ),

                  ProgramType::Unknown(7) => {
                        let ts = phdr.memsz() as usize;
                        tls_size = Some(ts);
                        load_tls(ts);
                  }

                  _ => {}
            }
      }

      let entry = paging::LAddr::from(elf.header().entry_point() as usize);
      (*entry, tls_size)
}
