use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

use quote::ToTokens;

use crate::parse::{ProtoItem, ProtoType};

pub fn gen(items: Vec<ProtoItem>, target_root: &Path) -> Result<(), Box<dyn Error>> {
    let mut map = HashMap::new();

    for item in items {
        let (mut t1, t2);
        let path = item.parent;
        let writer = match map.entry(path.clone()) {
            Entry::Occupied(ent) => {
                t1 = ent;
                t1.get_mut()
            }
            Entry::Vacant(ent) => {
                let file_path = target_root.join(&path);
                println!("{file_path:?}");

                fs::create_dir_all(file_path.parent().unwrap())?;
                let file = File::options()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(file_path)?;
                t2 = ent;
                t2.insert(BufWriter::new(file))
            }
        };
        match item.ty {
            ProtoType::Protocol(proto) => write!(writer, "{}", proto.quote()?)?,
            ProtoType::Item(item) => write!(writer, "{}", item.to_token_stream())?,
        }
    }
    map.into_iter()
        .try_for_each(|(_, mut writer)| writer.flush())?;
    Ok(())
}
