use std::{error::Error, io::Read, path::Path};

use quote::ToTokens;

fn parse_fn(func: syn::ItemFn) -> Result<Option<syn::ItemFn>, Box<dyn Error>> {
    for attr in func.attrs.iter() {
        let path = attr.path.to_token_stream().to_string();
        if &path == "syscall" {
            return Ok(Some(func));
        }
    }
    Ok(None)
}

fn parse_mod(is_syscall: bool, items: Vec<syn::Item>) -> Result<Vec<syn::ItemFn>, Box<dyn Error>> {
    let mut ret = Vec::new();
    for item in items {
        match item {
            syn::Item::Fn(func) if is_syscall => {
                if let Some(func) = parse_fn(func)? {
                    ret.push(func);
                }
            }
            syn::Item::Mod(syn::ItemMod {
                content: Some((_, sub_items)),
                ident,
                ..
            }) => ret.append(&mut parse_mod(&ident.to_string() == "syscall", sub_items)?),
            _ => {}
        }
    }
    Ok(ret)
}

fn parse_file(file: impl AsRef<Path>) -> Result<Vec<syn::ItemFn>, Box<dyn Error>> {
    println!("Parsing {:?}", file.as_ref());
    let is_syscall = file
        .as_ref()
        .as_os_str()
        .to_str()
        .map_or(false, |s| s.ends_with("syscall.rs"));
    let content = {
        let mut file = std::fs::File::open(file)?;
        let mut string = String::new();
        let _ = file.read_to_string(&mut string)?;
        string
    };
    let ast = syn::parse_file(&content)?;

    parse_mod(is_syscall, ast.items)
}

pub fn parse_dir(dir: std::fs::ReadDir) -> Result<Vec<syn::ItemFn>, Box<dyn Error>> {
    let mut ret = Vec::new();
    for ent in dir.flatten() {
        let ty = ent.file_type()?;
        let name = ent.file_name().to_string_lossy().to_string();
        if ty.is_dir() {
            let dir = std::fs::read_dir(ent.path())?;
            ret.append(&mut parse_dir(dir)?);
        } else if ty.is_file() && name.ends_with("rs") {
            ret.append(&mut parse_file(ent.path())?);
        }
    }
    Ok(ret)
}
