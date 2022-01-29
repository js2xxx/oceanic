use quote::{format_ident, ToTokens};
use syn::{parse::Parse, punctuated::Punctuated, *};

pub struct SyscallStub {
    num: LitInt,
    vis: Visibility,
    unsafety: Option<Token![unsafe]>,
    ident: Ident,
    args: Punctuated<FnArg, Token![,]>,
    output: ReturnType,
}

impl Parse for SyscallStub {
    fn parse(input: parse::ParseStream) -> Result<Self> {
        let num = input.parse::<LitInt>()?;
        input.parse::<Token![=>]>()?;
        let vis = input.parse::<Visibility>()?;
        let unsafety = input.parse::<Option<Token![unsafe]>>()?;
        input.parse::<Token![fn]>()?;
        let ident = input.parse::<Ident>()?;
        let args = {
            let args;
            parenthesized!(args in input);
            Punctuated::<FnArg, Token![,]>::parse_terminated(&args)?
        };
        let output = input.parse::<ReturnType>()?;

        Ok(SyscallStub {
            num,
            vis,
            unsafety,
            ident,
            args,
            output,
        })
    }
}

impl ToTokens for SyscallStub {
    fn to_tokens(&self, tokens: &mut __private::TokenStream2) {
        let SyscallStub {
            num,
            vis,
            unsafety,
            ident,
            args,
            output,
        } = self;
        let ty = match output {
            ReturnType::Default => parse_quote!(()),
            ReturnType::Type(_, ty) => ty.clone(),
        };

        let args_into = crate::wrap_args(
            &self.args,
            |a| match a {
                FnArg::Typed(PatType { pat, .. }) => match *pat {
                    Pat::Ident(ident) => {
                        let ret: Expr = parse_quote!(#ident.encode());
                        ret
                    }
                    _ => panic!("Function only receive typed args"),
                },
                _ => panic!("Function only receive typed args"),
            },
            Some(parse_quote!(0)),
        );

        let upper = ident.to_string().to_ascii_uppercase();
        let const_ident = format_ident!("FN_{}", upper);

        let out_fn: ItemFn = parse_quote! {
            #[cfg(feature = "call")]
            #vis #unsafety fn #ident (#args) -> crate::Result<#ty> {
                let arg = crate::Arguments {
                    fn_num: #num,
                    args: [#args_into],
                };
                let ret = unsafe { crate::call::raw::syscall(&arg) };
                ret.map(|val| <#ty as crate::SerdeReg>::decode(val))
            }
        };
        out_fn.to_tokens(tokens);

        let out_const: ItemConst = parse_quote! {
            pub const #const_ident: usize = #num;
        };
        out_const.to_tokens(tokens);
    }
}
