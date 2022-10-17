#![no_std]
#![no_main]
#![feature(slice_ptr_get)]

mod boot;

extern crate alloc;

async fn main() {
    unsafe { dldisconn() };
    log::debug!("Hello world!");

    solvent_std::env::args().for_each(|arg| log::debug!("{arg}"));

    solvent_async::test::test_disp().await;

    boot::mount();

    log::debug!("Goodbye!");
}

solvent_async::entry!(main, solvent_std);

#[link(name = "ldso")]
extern "C" {
    fn dldisconn();
}
