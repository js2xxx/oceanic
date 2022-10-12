#![no_std]
#![no_main]

extern crate alloc;

async fn main() {
    unsafe { dldisconn() };
    log::debug!("Hello world!");
    solvent_std::env::args().for_each(|arg| log::debug!("{arg}"));

    solvent_async::test::test_disp().await;
    log::debug!("Goodbye!");
}

solvent_async::entry!(main, solvent_std);

#[link(name = "ldso")]
extern "C" {
    fn dldisconn();
}
