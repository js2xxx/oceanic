use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rustc-link-arg=-z combreloc");
    Ok(())
}
