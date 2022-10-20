use std::fmt;

use syn::*;

use crate::types::Protocol;

#[derive(Debug)]
pub enum ProtoType {
    Protocol(Protocol),
    Item(Item),
}

pub struct ProtoItem {
    pub parent: std::path::PathBuf,
    pub ty: ProtoType,
}

impl fmt::Debug for ProtoItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProtoItem")
            .field("parent", &self.parent)
            .field(
                "ty",
                if matches!(self.ty, ProtoType::Protocol(..)) {
                    &"Protocol"
                } else {
                    &"Item"
                },
            )
            .finish()
    }
}

type FilePath = std::path::Path;

pub fn parse_root(dir: &FilePath) -> Result<Vec<ProtoItem>> {
    const NAME: &str = "mod.rs";
    let mod_path = dir.join(NAME);
    println!("cargo:rerun-if-changed={}", mod_path.to_str().unwrap());
    let content = std::fs::read_to_string(&mod_path).expect("Failed to read module file");
    let file = parse_file(&content)?;
    parse_mod_items(&mod_path, file.items, false)
}

fn parse_mod(
    mod_path: &FilePath,
    mod_dir: &FilePath,
    submod_dir: &FilePath,
    is_inline: bool,
    is_in_modrs: bool,
    module: ItemMod,
) -> Result<Vec<ProtoItem>> {
    match module.content {
        Some((_, items)) => parse_mod_items(mod_path, items, true),
        None => {
            let name = module.ident.to_string();
            let path = module.attrs.iter().find_map(|attr| {
                let attr = attr.parse_meta().ok()?;
                let path = match attr {
                    Meta::NameValue(MetaNameValue {
                        path,
                        lit: Lit::Str(lit),
                        ..
                    }) if path.is_ident("path") => lit.value(),
                    _ => return None,
                };
                let path = if is_inline && !is_in_modrs {
                    submod_dir.join(path)
                } else {
                    mod_dir.join(path)
                };
                Some(path)
            });
            let mut paths = path.into_iter().chain([
                submod_dir.join(name.clone() + ".rs"),
                submod_dir.join(name.clone() + "/mod.rs"),
            ]);
            let path = match paths.find(|path| path.exists()) {
                Some(path) => path,
                None => panic!("Failed to find module file {} in {:?}", name, mod_path),
            };
            println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
            let content = std::fs::read_to_string(&path).expect("Failed to read module file");
            let file = parse_file(&content)?;

            parse_mod_items(&path, file.items, false)
        }
    }
}

fn parse_mod_items(
    mod_path: &FilePath,
    items: Vec<Item>,
    is_inline: bool,
) -> Result<Vec<ProtoItem>> {
    let mut ret = Vec::new();
    for item in items {
        match item {
            Item::Mod(module) => {
                let temp;
                let mod_dir = mod_path.parent().unwrap();
                let is_modrs = mod_path.ends_with("mod.rs");
                let submod_dir = if is_modrs {
                    mod_dir
                } else {
                    temp = mod_dir.join(mod_path.file_stem().unwrap());
                    &temp
                };
                if module.content.is_none() {
                    ret.push(ProtoItem {
                        parent: mod_path.to_path_buf(),
                        ty: ProtoType::Item(Item::Mod(module.clone())),
                    });
                }
                ret.append(&mut parse_mod(
                    mod_path, mod_dir, submod_dir, is_inline, is_modrs, module,
                )?)
            }
            Item::Trait(t) => {
                let is_protocol = t.attrs.iter().any(|attr| {
                    matches!(
                        attr.parse_meta(),
                        Ok(Meta::List(MetaList { path, .. })) | Ok(Meta::Path(path))
                            if path.is_ident("protocol")
                    )
                });
                // println!("{:#?}, {is_protocol}", t.attrs);
                ret.push(ProtoItem {
                    parent: mod_path.to_path_buf(),
                    ty: if is_protocol {
                        ProtoType::Protocol(parse_quote!(#t))
                    } else {
                        ProtoType::Item(Item::Trait(t))
                    },
                })
            }
            Item::Verbatim(verb) => panic!("Verbatim detected: {:?}", verb),
            item => ret.push(ProtoItem {
                parent: mod_path.to_path_buf(),
                ty: ProtoType::Item(item),
            }),
        }
    }
    Ok(ret)
}
