use std::{error::Error, fs, io::Read, os::unix::prelude::OsStringExt, path::Path};

use bootfs::gen::{Content, Entry};

fn parse_file(path: impl AsRef<Path>, name: Vec<u8>) -> Result<Entry, Box<dyn Error>> {
    let mut content = vec![];
    fs::File::open(path)?.read_to_end(&mut content)?;
    Ok(Entry {
        name,
        content: Content::File(content),
    })
}

fn parse_dir(path: impl AsRef<Path>, name: Vec<u8>) -> Result<Entry, Box<dyn Error>> {
    let content = fs::read_dir(path)?
        .flatten()
        .try_fold(Vec::<Entry>::new(), |mut acc, ent| {
            let ty = ent.file_type()?;
            if ty.is_file() {
                acc.push(parse_file(ent.path(), ent.file_name().into_vec())?);
            } else if ty.is_dir() {
                acc.push(parse_dir(ent.path(), ent.file_name().into_vec())?);
            }
            Ok::<_, Box<dyn Error>>(acc)
        })?;
    Ok(Entry {
        name,
        content: Content::Directory(content),
    })
}

pub fn parse(root: impl AsRef<Path>) -> Result<Entry, Box<dyn Error>> {
    parse_dir(root, "bootfs".as_bytes().to_owned())
}
