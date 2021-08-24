//! The FS module of H2O's boot loader.

use bitop_ex::BitOpEx;

use core::alloc::Layout;
use core::mem::size_of;
use object::read::elf::*;
use object::{elf::*, Endianness};
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

      // We need to manually allocate the memory for the kernel file instead of creating a new
      // `Vec<u8>` because we need to align the file properly and the latter is badly aligned.
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

/// Transform the flags of a ELF program header into the attribute of a paging entry.
///
/// In this case, we only focus on the read/write-ability and executability.
fn flags_to_pg_attr(flags: u32) -> paging::Attr {
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
            "file::load_prog: flags = {:?}, virt = {:?}, phys = {:?}, fsize = {:x}, msize = {:x}",
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
            // Must clear the memory, otherwise some static variables will be uninitialized.
            unsafe { core::ptr::write_bytes(*phys.to_laddr(crate::mem::EFI_ID_OFFSET), 0, extra) };

            let virt = paging::LAddr::from(vend)..paging::LAddr::from(vend + extra);
            crate::mem::maps(syst, virt, phys, pg_attr).expect("Failed to map virtual memory");
      }
}

/// Load a Processor-Local Storage (PLS) segment.
fn load_pls(syst: &SystemTable<Boot>, size: usize, align: usize) -> Layout {
      log::trace!("file::map: loading TLS: size = {:?}", size);

      let layout = Layout::from_size_align(size, align)
            .expect("Failed to create the PLS layout")
            .pad_to_align();
      let size = layout.size();

      let pls = {
            let alloc_size = size + size_of::<*mut usize>();
            let laddr = crate::mem::alloc(syst)
                  .alloc_n(alloc_size.div_ceil_bit(paging::PAGE_SHIFT))
                  .expect("Failed to allocate memory for PLS")
                  .to_laddr(crate::mem::EFI_ID_OFFSET);
            *laddr
      };

      unsafe {
            let self_ptr = pls.add(size).cast::<usize>();
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
      }

      layout
}

/// Map a ELF executable into the memory.
///
/// # Returns
///
/// This function returns a tuple with 2 elements where the first element is the entry point of the
/// ELF executable and the second element is the TLS size of it.
pub fn map_elf(syst: &SystemTable<Boot>, data: &[u8]) -> (*mut u8, Option<Layout>) {
      log::trace!(
            "file::map: syst = {:?}, data = {:?}",
            syst as *const _,
            data.as_ptr()
      );

      let en = Endianness::default();
      let elf = ElfFile64::<'_, Endianness>::parse(data).expect("Failed to map ELF64 file");
      assert!(elf.raw_header().is_type_64());

      let mut pls_layout = None;
      for phdr in elf.raw_segments() {
            match phdr.p_type(en) {
                  PT_LOAD => load_prog(
                        syst,
                        phdr.p_flags(en),
                        paging::LAddr::from(phdr.p_vaddr(en) as usize),
                        paging::PAddr::new(
                              unsafe { data.as_ptr().add(phdr.p_offset(en) as usize) } as usize,
                        ),
                        (phdr.p_filesz(en) as usize).round_up_bit(paging::PAGE_SHIFT),
                        (phdr.p_memsz(en) as usize).round_up_bit(paging::PAGE_SHIFT),
                  ),

                  PT_TLS => {
                        let ts = phdr.p_memsz(en) as usize;
                        pls_layout = Some(load_pls(syst, ts, phdr.p_align(en) as usize));
                  }

                  _ => {}
            }
      }

      let entry = paging::LAddr::from(elf.raw_header().e_entry(en) as usize);
      (*entry, pls_layout)
}
