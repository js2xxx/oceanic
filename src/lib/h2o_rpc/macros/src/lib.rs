#![feature(iterator_try_collect)]

mod protocol;
mod serde_packet;

use proc_macro::TokenStream;

#[proc_macro_derive(SerdePacket)]
pub fn derive_serde_packet(input: TokenStream) -> TokenStream {
    match serde_packet::derive(input) {
        Ok(output) => output,
        Err(err) => err.to_compile_error().into(),
    }
}
