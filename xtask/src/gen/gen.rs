use std::iter;

use quote::{__private::Span, format_ident, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    parse_quote,
    punctuated::{Pair, Punctuated},
    Block, Error, Expr, ExprArray, ExprBlock, FnArg, Ident, ItemFn, Pat, PatType, Result,
    Signature, Token, Type,
};

struct UserPtrType {
    is_mut: bool,
    ty: Type,
}

impl Parse for UserPtrType {
    fn parse(input: ParseStream) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        if &ident.to_string() != "UserPtr" {
            return Err(Error::new(
                Span::call_site(),
                "Only `UserPtr` supports this",
            ));
        }
        input.parse::<Token![<]>()?;
        let attr = input.parse::<Ident>()?;
        let is_mut = match &*attr.to_string() {
            "In" => false,
            "Out" | "InOut" => true,
            _ => return Err(Error::new(Span::call_site(), "Invalid attribute")),
        };
        input.parse::<Token![,]>()?;
        let ty = input.parse::<Type>()?;
        input.parse::<Token![>]>()?;
        Ok(UserPtrType { is_mut, ty })
    }
}

impl UserPtrType {
    fn process(arg: &FnArg) -> FnArg {
        match &arg {
            FnArg::Typed(t @ PatType { ty, .. }) => {
                match syn::parse2::<UserPtrType>(ty.to_token_stream()) {
                    Ok(UserPtrType { is_mut, ty }) => {
                        let token: Type = if is_mut {
                            parse_quote!(*mut #ty)
                        } else {
                            parse_quote!(*const #ty)
                        };
                        FnArg::Typed(PatType {
                            ty: Box::new(token),
                            ..t.clone()
                        })
                    }
                    Err(_) => arg.clone(),
                }
            }
            _ => arg.clone(),
        }
    }
}

fn wrapper_stub(func: &ItemFn) -> Result<ExprBlock> {
    let ident = &func.sig.ident;
    let wrapper_ident = format_ident!("wrapper_{}", ident);
    let ret = parse_quote! {
        {
            extern "C" {
                fn #wrapper_ident (
                    a: usize,
                    b: usize,
                    c: usize,
                    d: usize,
                    e: usize,
                ) -> usize;
            }
            #wrapper_ident
        }
    };
    Ok(ret)
}

pub fn wrapper_stubs(funcs: &[ItemFn]) -> Result<ExprArray> {
    let elem = funcs.iter().map(wrapper_stub).collect::<Result<Vec<_>>>()?;
    let mut elems = Punctuated::<Expr, Token![,]>::new();
    elems.extend(
        elem.into_iter()
            .map(|item| Pair::Punctuated(Expr::Block(item), <Token![,]>::default())),
    );
    Ok(ExprArray {
        attrs: Default::default(),
        bracket_token: Default::default(),
        elems,
    })
}

pub fn call_stub(num: usize, func: ItemFn) -> Result<[ItemFn; 2]> {
    let inputs = {
        let mut ret = Punctuated::<FnArg, Token![,]>::new();
        let items = func.sig.inputs.iter().map(UserPtrType::process);
        ret.extend(items.map(|arg| Pair::Punctuated(arg, <Token![,]>::default())));
        ret
    };
    let encode_args = {
        let mut ret = Punctuated::<Expr, Token![,]>::new();
        let fn_num: Expr = parse_quote!(#num);
        let items = inputs
            .iter()
            .map(|arg| match arg {
                FnArg::Typed(PatType { pat, ty, .. }) => match **pat {
                    Pat::Ident(ref ident) => parse_quote!(<#ty as SerdeReg>::encode(#ident)),
                    _ => panic!("Function only receive typed args"),
                },
                _ => panic!("Function only receive typed args"),
            })
            .chain(iter::repeat_with(|| parse_quote!(0usize)))
            .take(5);
        ret.extend(items.map(|arg| Pair::Punctuated(arg, <Token![,]>::default())));
        ret.insert(0, fn_num);
        ret
    };
    let pass_args = {
        let mut ret = Punctuated::<Expr, Token![,]>::new();
        let items = inputs.iter().map(|arg| match arg {
            FnArg::Typed(PatType { pat, .. }) => match **pat {
                Pat::Ident(ref ident) => parse_quote!(#ident),
                _ => panic!("Function only receive typed args"),
            },
            _ => panic!("Function only receive typed args"),
        });
        ret.extend(items.map(|arg| Pair::Punctuated(arg, <Token![,]>::default())));
        ret
    };
    let c_out_body: Block = parse_quote! {
        {
            #[allow(unused_unsafe)]
            unsafe { raw::syscall(#encode_args) }
        }
    };

    let c_ident = format_ident!("sv_{}", &func.sig.ident);
    let rust_out_body: Block = parse_quote! {
        {
            let ret = #c_ident (#pass_args);
            SerdeReg::decode(ret)
        }
    };

    let mut attrs = func.attrs;
    attrs.retain(|attr| attr.path.to_token_stream().to_string() != "syscall");

    let mut c_attrs = attrs.clone();
    c_attrs.push(parse_quote!(#[no_mangle]));
    let c_func = ItemFn {
        sig: Signature {
            inputs: inputs.clone(),
            abi: Some(parse_quote!(extern "C")),
            output: parse_quote!(-> usize),
            ident: c_ident,
            ..func.sig.clone()
        },
        vis: parse_quote!(pub),
        block: Box::new(c_out_body),
        attrs: c_attrs,
    };

    let rust_func = ItemFn {
        sig: Signature { inputs, ..func.sig },
        vis: parse_quote!(pub),
        block: Box::new(rust_out_body),
        attrs,
    };

    Ok([c_func, rust_func])
}
