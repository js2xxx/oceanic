use core::mem::size_of;
use elf_rs::*;
use uefi::prelude::*;
use uefi::proto::loaded_image::*;
use uefi::proto::media::{file::File, *};

static mut LOCAL_VOL: Option<file::Directory> = None;

pub fn init(img: Handle, syst: &SystemTable<Boot>) {
      let bs = syst.boot_services();

      let local_img = bs
            .handle_protocol::<LoadedImage>(img)
            .expect_success("Failed to locate loaded image protocol");
      let fs = bs
            .handle_protocol::<fs::SimpleFileSystem>(unsafe { &*local_img.get() }.device())
            .expect_success("Failed to locate file system protocol");

      unsafe {
            LOCAL_VOL = Some((&mut *fs.get())
                  .open_volume()
                  .expect_success("Failed to open the local volume"));
      }
}

pub fn load(syst: &SystemTable<Boot>, filename: &str) -> (paging::PAddr, usize) {
      log::trace!("file::load: filename = {}", filename);

      let mut volume = unsafe {
            LOCAL_VOL
                  .take()
                  .expect("The local volume should be initialized")
      };

      let mut kfile = volume
            .open(filename, file::FileMode::Read, file::FileAttribute::empty())
            .expect_success("Failed to open kernel file");

      let ksize = {
            let mut finfo_buffer = alloc::vec![0; super::mem::PAGE_SIZE];
            let finfo: &mut file::FileInfo = kfile
                  .get_info(&mut finfo_buffer)
                  .expect_success("Failed to get kernel file information");

            finfo.file_size() as usize
      };

      let ksize_aligned = round_up_p2(ksize, paging::PAGE_SIZE);
      let kfile_addr = crate::mem::alloc(syst)
            .alloc_n(ksize_aligned >> paging::PAGE_SHIFT)
            .expect("Failed to allocate memory");
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

      unsafe { LOCAL_VOL = Some(volume) };
      (kfile_addr, ksize)
}

#[inline]
fn round_up_p2(x: usize, u: usize) -> usize {
      (x.wrapping_sub(1) | (u - 1)).wrapping_add(1)
}

#[inline]
fn round_down_p2(x: usize, u: usize) -> usize {
      x & !(u - 1)
}

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

pub fn map(syst: &SystemTable<Boot>, data: &[u8]) -> (*mut u8, Option<usize>) {
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

      let u = elf.program_headers();

      let mut tls_size = None;
      for phdr in elf.program_headers() {
            match phdr.ph_type() {
                  ProgramType::LOAD => {
                        let fsize = round_up_p2(phdr.filesz() as usize, paging::PAGE_SIZE);
                        let msize = round_up_p2(phdr.memsz() as usize, paging::PAGE_SIZE);
                        log::trace!(
                              "file::map: loading PHDR: flags = {:?}, fsize = {:?}, msize = {:?}",
                              phdr.flags(),
                              fsize,
                              msize
                        );

                        let pg_attr = flags_to_pg_attr(phdr.flags());
                        let (vstart, vend) = (phdr.vaddr() as usize, phdr.vaddr() as usize + fsize);

                        if fsize > 0 {
                              let phys = paging::PAddr::new(unsafe {
                                    data.as_ptr().add(phdr.offset() as usize)
                              }
                                    as usize);
                              let virt = paging::LAddr::from(vstart)..paging::LAddr::from(vend);
                              crate::mem::maps(syst, virt, phys, pg_attr)
                                    .expect("Failed to map virtual memory");
                        }

                        if msize > fsize {
                              let extra = msize - fsize;
                              let phys = crate::mem::alloc(syst)
                                    .alloc_n(extra >> paging::PAGE_SHIFT)
                                    .expect("Failed to allocate extra memory");
                              let virt =
                                    paging::LAddr::from(vend)..paging::LAddr::from(vend + extra);
                              crate::mem::maps(syst, virt, phys, pg_attr)
                                    .expect("Failed to map virtual memory");
                        }
                  }
                  ProgramType::Unknown(7) => {
                        let ts = phdr.memsz() as usize;
                        tls_size = Some(ts);

                        log::trace!(
                              "file::map: loading TLS: flags = {:?}, size = {:?}",
                              phdr.flags(),
                              ts,
                        );

                        unsafe {
                              let tls_vec = alloc::vec::Vec::<u8>::with_capacity(
                                    ts + size_of::<*mut usize>(),
                              );
                              let (tls, _, _) = tls_vec.into_raw_parts();
                              let self_ptr = tls.add(ts).cast::<usize>();
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
                  _ => {}
            }
      }

      (elf.header().entry_point() as *mut u8, tls_size)
}
