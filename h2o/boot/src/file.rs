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

pub fn load(filename: &str) -> alloc::vec::Vec<u8> {
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

      let mut kfile_data = alloc::vec![0; ksize];
      match kfile
            .into_type()
            .expect_success("Failed to deduce kernel file type")
      {
            file::FileType::Regular(mut kfile) => assert!(
                  kfile.read(&mut kfile_data)
                        .expect_success("Failed to read kernel file")
                        == ksize,
                  "Failed to read whole kernel file"
            ),
            _ => panic!("Kernel file should be a regular file"),
      }

      unsafe { LOCAL_VOL = Some(volume) };
      kfile_data
}

pub fn map(data: &[u8]) -> (*mut u8, usize) {
      let elf = Elf::from_bytes(data).expect("Failed to map ELF file");
      let elf = match elf {
            Elf::Elf64(e) => e,
            _ => panic!("ELF64 file accepted only"),
      };

      let u = elf.program_headers();
      log::info!("{:?}", u[0]);

      for prog in elf.program_headers() {
            match prog.ph_type() {
                ProgramType::LOAD => {
                      
                }
                ProgramType::Unknown(7) => {}
                _ => {}
            }
      }
      todo!()
}
