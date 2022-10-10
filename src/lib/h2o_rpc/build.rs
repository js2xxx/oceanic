use std::{path::PathBuf, str::FromStr};

fn main() {
    solvent_rpc_gen::generate(
        &PathBuf::from_str("imp").unwrap(),
        &PathBuf::from_str("target").unwrap(),
    )
}
