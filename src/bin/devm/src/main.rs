#![no_std]
#![no_main]

mod device;
mod driver;

use alloc::vec;

use solvent::prelude::{Channel, Phys};
use solvent_fs::{process::Process, rpc::RpcNode, spawner};
use solvent_rpc::{
    io::{self, file::PhysOptions, OpenOptions},
    sync::Client,
};

extern crate alloc;

async fn main() {
    let drvhost = driver_host().expect("Failed to get driver host");

    let root_driver = "boot/drv/libpc.so";

    let bootfs = solvent_fs::open_dir("/boot", OpenOptions::READ).expect("Failed to open bootfs");
    let bootfs = bootfs.into_async().expect("Failed to get loader");

    let mut vfs = vec![];
    solvent_fs::fs::local()
        .export(&mut vfs)
        .expect("Failed to export vfs");
    let (instance, server) = Channel::new();

    vfs.push(("use/devm".into(), instance.into()));

    let mut task = Process::builder()
        .executable(drvhost, "drvhost")
        .expect("Failed to set executable")
        .arg(root_driver)
        .load_dirs(vec![bootfs])
        .expect("Failed to set load dirs")
        .local_fs(vfs)
        .build()
        .await
        .expect("Failed to build the process");
    log::debug!("Starting the root driver");

    let node = RpcNode::new(|server, _| async move { device::handle_driver(server).await });
    node.open_conn(spawner(), Default::default(), server);

    let ret = task.ajoin().await.expect("Failed to join the process");
    assert_eq!(ret, 0);
}

fn driver_host() -> Result<Phys, io::Error> {
    let drvhost = solvent_fs::open(
        "boot/bin/drvhost",
        OpenOptions::READ | OpenOptions::EXECUTE | OpenOptions::EXPECT_FILE,
    )?;
    let drvhost = drvhost.phys(PhysOptions::Copy)??;
    Ok(drvhost)
}

solvent_async::entry!(main, solvent_std, Some(1));
