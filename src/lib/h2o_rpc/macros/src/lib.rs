#![feature(box_into_inner)]
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

#[proc_macro_attribute]
pub fn protocol(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as protocol::Protocol);
    match protocol::gen(args, input) {
        Ok(output) => output,
        Err(err) => err.to_compile_error().into(),
    }
}
