#![no_std]
#![no_main]
#![feature(slice_ptr_get)]

mod boot;

extern crate alloc;

async fn main() {
    unsafe { dldisconn() };
    log::debug!("Hello world!");

    boot::mount();

    solvent_std::env::args().for_each(|arg| log::debug!("{arg}"));

    solvent_async::test::test_disp().await;

    log::debug!("Goodbye!");
}

solvent_async::entry!(main, solvent_std, None);

#[link(name = "ldso")]
extern "C" {
    fn dldisconn();
}
