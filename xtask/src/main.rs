#![feature(exit_status_error)]

use std::error::Error;

use structopt::StructOpt;

mod dist;

#[derive(Debug, StructOpt)]
enum Cmd {
    Dist(dist::Dist),
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Cmd::from_args();
    match args {
        Cmd::Dist(dist) => dist.build(),
    }
}
