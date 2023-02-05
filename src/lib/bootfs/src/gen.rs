use std::{collections::VecDeque, io::Write, mem, vec::Vec};

use plain::Plain;

use crate::{
    BootfsHeader, EntryType, ENTRY_LAYOUT, HEADER_SIZE, MAX_NAME_LEN, PAGE_LAYOUT, VERSION,
};

pub enum Content {
    File(Vec<u8>),
    Directory(Vec<Entry>),
}

pub struct Entry {
    pub name: Vec<u8>,
    pub content: Content,
}

fn split(input: &Entry, entries: &mut Vec<super::Entry>, contents: &mut Vec<Vec<u8>>) {
    let mut q = VecDeque::new();
    q.push_back(input);

    let mut ent_index = 0;
    while let Some(entry) = q.pop_front() {
        let mut name = entry.name.clone();
        assert!(name.len() < MAX_NAME_LEN, "Name too long");
        name.resize(MAX_NAME_LEN, 0);

        match &entry.content {
            Content::File(content) => {
                let entry = super::Entry {
                    version: VERSION,
                    name: name.try_into().expect("Name too long"),
                    ty: crate::EntryType::File,
                    offset: 0,
                    len: content.len(),
                };
                ent_index += 1;
                entries.push(entry);
                contents.push(content.clone());
            }
            Content::Directory(ent) => {
                let entry = super::Entry {
                    version: VERSION,
                    name: name.try_into().expect("Name too long"),
                    ty: crate::EntryType::Directory,
                    offset: HEADER_SIZE + (ent_index + q.len() + 1) * mem::size_of::<usize>(),
                    len: ent.len() * mem::size_of::<usize>(),
                };
                ent_index += 1;
                entries.push(entry);
                ent.iter().for_each(|ent| q.push_back(ent))
            }
        }
    }
}

fn write_typed<T: ?Sized + Plain>(
    data: &T,
    size: usize,
    output: &mut impl Write,
) -> anyhow::Result<()> {
    let alsize = mem::size_of_val(data);
    let size = alsize.max(size);

    // SAFETY: The data is `Plain`.
    output.write_all(unsafe { plain::as_bytes(data) })?;
    for _ in alsize..size {
        output.write_all(&[0])?;
    }

    Ok(())
}

pub fn generate(input: &Entry, output: &mut impl Write) -> anyhow::Result<()> {
    let mut entries = Vec::new();
    let mut contents = Vec::new();
    split(input, &mut entries, &mut contents);

    let mut len = 0;
    // Generate the header.
    {
        let bootfs_header = BootfsHeader {
            version: VERSION,
            num_entries: entries.len(),
            root_dir_offset: HEADER_SIZE + mem::size_of::<usize>(),
            root_dir_len: entries[0].len,
        };
        write_typed(&bootfs_header, HEADER_SIZE, output)?;
        len += HEADER_SIZE;
    }

    // Generate the entry offset array.
    let ent_size = {
        let off_start = HEADER_SIZE;
        let ent_start = (off_start + entries.len() * mem::size_of::<usize>())
            .next_multiple_of(PAGE_LAYOUT.align());

        let ent_size = ENTRY_LAYOUT.pad_to_align().size();
        for i in 0..entries.len() {
            write_typed(&(ent_start + i * ent_size), mem::size_of::<usize>(), output)?;
            len += mem::size_of::<usize>();
        }

        let aligned_len = len.next_multiple_of(PAGE_LAYOUT.align());
        for _ in len..aligned_len {
            output.write_all(&[0])?;
        }
        len = aligned_len;
        ent_size
    };

    // Generate the entry metadata array.
    let file_offsets = {
        let mut file_offsets = Vec::new();

        let mut offset = len + (entries.len() * ent_size).next_multiple_of(PAGE_LAYOUT.align());

        for entry in entries.iter_mut() {
            if entry.ty == EntryType::File {
                entry.offset = offset;
                file_offsets.push(offset);
                offset += entry.len.next_multiple_of(PAGE_LAYOUT.align());
            }

            write_typed(entry, ent_size, output)?;
            len += ent_size;
        }

        file_offsets
    };

    // Copy file contents.
    if !contents.is_empty() {
        assert_eq!(file_offsets.len(), contents.len());

        let first_len = *file_offsets.first().unwrap();
        for _ in len..first_len {
            output.write_all(&[0])?;
        }

        let end_offset = (file_offsets.last().unwrap() + contents.last().unwrap().len())
            .next_multiple_of(PAGE_LAYOUT.align());
        let end_content = Vec::new();

        let mut iter = file_offsets
            .iter()
            .zip(contents.iter())
            .chain(Some((&end_offset, &end_content)))
            .peekable();

        while let (Some((&offset, content)), Some((&next_offset, _))) = (iter.next(), iter.peek()) {
            output.write_all(content)?;
            for _ in (offset + content.len())..next_offset {
                output.write_all(&[0])?;
            }
        }
    }

    Ok(())
}
