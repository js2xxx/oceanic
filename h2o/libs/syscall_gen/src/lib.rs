mod syscall_fn;

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{punctuated::Punctuated, *};

fn wrap_args<T, F>(
    args: &Punctuated<FnArg, Token![,]>,
    f: F,
    def: Option<T>,
) -> Punctuated<T, Token![,]>
where
    T: Clone,
    F: Fn(FnArg) -> T,
{
    match def {
        Some(def) => args
            .iter()
            .map(|a| f(a.clone()))
            .chain(core::iter::repeat(def))
            .take(5)
            .collect(),
        None => args.iter().map(|a| f(a.clone())).collect(),
    }
}

#[proc_macro_attribute]
pub fn syscall(args: TokenStream, item_fn: TokenStream) -> TokenStream {
    assert!(
        args.to_string().is_empty(),
        "This macro don't receive any arguments"
    );
    let func = parse_macro_input!(item_fn as syscall_fn::SyscallFn);

    quote!(#func).into()
}
