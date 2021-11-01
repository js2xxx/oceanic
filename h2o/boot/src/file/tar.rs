use alloc::{string::String, vec::Vec};
use core::ops::Deref;

use bitop_ex::BitOpEx;
use static_assertions::const_assert;

const BLK_SHIFT: usize = 9;
const BLK_SIZE: usize = 1 << BLK_SHIFT;

#[derive(Debug)]
pub struct Files<'a>(Vec<(String, &'a [u8])>);

impl<'a> Files<'a> {
    pub fn find<S>(&self, name: S) -> &'a [u8]
    where
        S: AsRef<str>,
    {
        self.0
            .iter()
            .find_map(|(nm, data)| (nm.starts_with(name.as_ref())).then_some(data.clone()))
            .expect("Failed to find file")
    }
}

impl<'a> Deref for Files<'a> {
    type Target = [(String, &'a [u8])];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[allow(dead_code)]
#[derive(Debug)]
#[repr(u8)]
enum FileType {
    Regular = b'0',         // regular file
    ARegular = b'\0',       // regular file
    Link = b'1',            // link
    Symbol = b'2',          // reserved
    CharDev = b'3',         // character special
    BlockDev = b'4',        // block special
    Directory = b'5',       // directory
    FifoDev = b'6',         // FIFO special
    _Reserved = b'7',       // reserved
    ExtHeader = b'x',       // Extended header referring to the next file in the archive
    GlobalExtHeader = b'g', // Global extended header
}

#[derive(Debug)]
#[repr(C)]
struct TarFileHeader {
    name: [u8; 100],
    mode: [u8; 8],
    uid: [u8; 8],
    gid: [u8; 8],
    size: [u8; 12],
    mtime: [u8; 12],
    chksum: [u8; 8],
    typeflag: FileType,
    linkname: [u8; 100],
    magic: [u8; 6],
    version: [u8; 2],
    uname: [u8; 32],
    gname: [u8; 32],
    devmajor: [u8; 8],
    devminor: [u8; 8],
    prefix: [u8; 155],
    _padding: [u8; 12],
}
const_assert!(core::mem::size_of::<TarFileHeader>() == BLK_SIZE);

fn get_size(mut raw: &[u8]) -> usize {
    while raw.ends_with(&[b'\0']) {
        raw = &raw[..raw.len() - 1];
    }

    raw.iter().fold(0, |acc, c| acc * 8 + ((c - b'0') as usize))
}

pub fn untar(mut data: &[u8]) -> Files {
    let next_ptr = |data: &mut &[u8], size: usize| {
        let ret = data.as_ptr();
        *data = &data[size..];
        ret
    };

    let mut files = Vec::new();
    loop {
        if data.len() < BLK_SIZE {
            break Files(files);
        }

        let header = unsafe { &*next_ptr(&mut data, BLK_SIZE).cast::<TarFileHeader>() };
        if &header.magic != b"ustar " {
            break Files(files);
        }

        let size = get_size(&header.size);
        let file_data = unsafe {
            core::slice::from_raw_parts(next_ptr(&mut data, size.round_up_bit(BLK_SHIFT)), size)
        };
        let name = String::from_utf8_lossy(&header.name).into_owned();

        files.push((name, file_data));
    }
}
