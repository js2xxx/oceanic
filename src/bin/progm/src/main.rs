#![no_std]
#![no_main]
#![feature(slice_ptr_get)]

mod boot;
mod com;

use alloc::vec;

use solvent_fs::process::Process;
use solvent_rpc::{io::OpenOptions, sync::Client};

extern crate alloc;

async fn main() {
    unsafe { dldisconn() };
    log::debug!("Hello world!");

    boot::mount();

    solvent_std::env::args().for_each(|arg| log::debug!("{arg}"));

    solvent_async::test::test_disp().await;

    com::get_boot_coms().await.expect("failed to get boot coms");

    let bootfs = solvent_fs::open_dir("/boot", OpenOptions::READ).expect("Failed to open bootfs");
    let bootfs = bootfs.into_async().expect("Failed to get loader");

    let devm =
        solvent_fs::loader::get_object_from_dir(solvent_async::dispatch(), &bootfs, "bin/devm")
            .await
            .expect("Failed to get executable");

    let mut vfs = vec![];
    solvent_fs::fs::local()
        .export(&mut vfs)
        .expect("Failed to export vfs");

    let mut task = Process::builder()
        .executable(devm, "devm")
        .expect("Failed to add executable")
        .load_dirs(vec![bootfs])
        .expect("Failed to add loader client")
        .local_fs(vfs)
        .build()
        .await
        .expect("Failed to build a process");

    log::debug!("Waiting for devm");
    let retval = task.ajoin().await.expect("Failed to wait for devm");
    assert_eq!(retval, 0, "The process failed: {retval:#x}");

    log::debug!("Goodbye!");
}

solvent_async::entry!(main, solvent_std, None);

#[link(name = "ldso")]
extern "C" {
    fn dldisconn();
}
