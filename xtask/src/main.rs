#![feature(exit_status_error)]

use structopt::StructOpt;

mod check;
mod dist;
mod gen;
const DEBUG_DIR: &str = "debug";

const H2O_BOOT: &str = "h2o/boot";
const H2O_KERNEL: &str = "h2o/kernel";
const H2O_TINIT: &str = "h2o/tinit";
const H2O_SYSCALL: &str = "h2o/libs/syscall";

const OC_LIB: &str = "src/lib";
const OC_BIN: &str = "src/bin";
const OC_DRV: &str = "src/drv";

const BOOTFS: &str = "target/bootfs";

#[derive(Debug, StructOpt)]
enum Cmd {
    Dist(dist::Dist),
    Check,
}

fn main() -> anyhow::Result<()> {
    let args = Cmd::from_args();
    match args {
        Cmd::Dist(dist) => dist.build(),
        Cmd::Check => check::check(),
    }
}
