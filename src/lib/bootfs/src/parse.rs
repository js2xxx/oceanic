use core::mem;

use either::Either;
use plain::Plain;

use crate::{MAX_NAME_LEN, VERSION};

#[derive(Debug, Copy, Clone)]
pub struct Directory<'a> {
    image: &'a [u8],
    dir: &'a [u8],
}

impl<'a> Directory<'a> {
    pub fn root(image: &'a [u8]) -> Option<Self> {
        let header = if image[..4] == VERSION.to_ne_bytes() {
            crate::BootfsHeader::from_bytes(&image[..mem::size_of::<crate::BootfsHeader>()]).ok()?
        } else {
            return None;
        };
        let root_dir = &image[header.root_dir_offset..][..header.root_dir_len];
        Some(Directory {
            image,
            dir: root_dir,
        })
    }

    pub fn iter(self) -> DirIter<'a> {
        DirIter {
            image: self.image,
            rem: self.dir,
        }
    }

    pub fn image(&self) -> &'a [u8] {
        self.image
    }

    pub fn get(self, name: &[u8]) -> Option<Entry<'a>> {
        self.iter().find(|ent| ent.name_eq(name))
    }

    pub fn find(self, path: &[u8], separator: u8) -> Option<&'a [u8]> {
        let mut dir = self;
        let mut names = path.split(|&b| b == separator);
        loop {
            let entry: Entry<'a> = dir.get(names.next()?)?;
            dir = match entry.content() {
                Either::Left(content) => break Some(content),
                Either::Right(dir) => dir,
            };
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct DirIter<'a> {
    image: &'a [u8],
    rem: &'a [u8],
}

impl<'a> Iterator for DirIter<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rem.len() < mem::size_of::<usize>() {
            return None;
        }
        let offset;
        (offset, self.rem) = self.rem.split_at(mem::size_of::<usize>());
        let offset = usize::from_ne_bytes(offset.try_into().unwrap());

        let entry = &self.image[offset..][..mem::size_of::<super::Entry>()];
        if entry[..4] != VERSION.to_ne_bytes() {
            return None;
        }

        let ret = crate::Entry::from_bytes(entry).ok()?;
        Some(Entry {
            image: self.image,
            metadata: *ret,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Entry<'a> {
    image: &'a [u8],
    metadata: super::Entry,
}

impl<'a> Entry<'a> {
    pub fn name_eq(&self, name: &[u8]) -> bool {
        let len = name.len().min(MAX_NAME_LEN - 1);
        &self.metadata.name[..len] == name && self.metadata.name[len] == b'\0'
    }

    pub fn metadata(&self) -> &super::Entry {
        &self.metadata
    }

    pub fn content(self) -> Either<&'a [u8], Directory<'a>> {
        let content = &self.image[self.metadata.offset..][..self.metadata.len];
        match self.metadata.ty {
            crate::EntryType::File => Either::Left(content),
            crate::EntryType::Directory => {
                assert!(content.len() & 7 == 0);
                Either::Right(Directory {
                    image: self.image,
                    dir: content,
                })
            }
        }
    }
}
