#![feature(exit_status_error)]

use std::error::Error;

use structopt::StructOpt;

mod check;
mod dist;

const H2O_BOOT: &str = "h2o/boot";
const H2O_KERNEL: &str = "h2o/kernel";
const H2O_TINIT: &str = "h2o/tinit";

#[derive(Debug, StructOpt)]
enum Cmd {
    Dist(dist::Dist),
    Check,
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args = Cmd::from_args();
    match args {
        Cmd::Dist(dist) => dist.build(),
        Cmd::Check => check::check(),
    }
}
