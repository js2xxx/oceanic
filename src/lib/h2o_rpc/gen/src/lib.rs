#![feature(box_into_inner)]

use std::path::Path;

mod gen;
mod parse;
mod resolve;
mod types;

pub fn generate(src: &Path, dst: &Path) {
    let mut items = parse::parse_root(src).expect("Failed to parse the directory");
    resolve::resolve(&mut items).expect("Failed to resolve dependencies");
    gen::gen(items, dst).expect("Failed to write to files");
}
