use quote::{format_ident, ToTokens};
use syn::parse::Parse;
use syn::punctuated::Punctuated;
use syn::*;

pub struct SyscallFn {
      vis: Visibility,
      unsafety: Option<Token![unsafe]>,
      ident: Ident,
      args: Punctuated<FnArg, Token![,]>,
      output: ReturnType,
      body: Block,
}

impl Parse for SyscallFn {
      fn parse(input: parse::ParseStream) -> Result<Self> {
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
            let body = input.parse::<Block>()?;
            Ok(SyscallFn {
                  vis,
                  unsafety,
                  ident,
                  args,
                  output,
                  body,
            })
      }
}

impl ToTokens for SyscallFn {
      fn to_tokens(&self, tokens: &mut __private::TokenStream2) {
            let SyscallFn {
                  vis,
                  unsafety,
                  ident,
                  args,
                  output,
                  body,
            } = self;
            let ty = match output {
                  ReturnType::Default => parse_quote!(()),
                  ReturnType::Type(_, ty) => ty.clone(),
            };

            let orig: ItemFn = parse_quote! {
                  #vis #unsafety fn #ident (#args) -> solvent::Result<#ty> #body
            };
            orig.to_tokens(tokens);

            let wrapper_ident = format_ident!("wrapper_{}", self.ident);

            let wrapper_args = crate::wrap_args(
                  &self.args,
                  |a| match a {
                        FnArg::Typed(PatType {
                              attrs,
                              pat,
                              colon_token,
                              ..
                        }) => FnArg::Typed(PatType {
                              attrs,
                              pat,
                              colon_token,
                              ty: parse_quote!(usize),
                        }),
                        a => a,
                  },
                  Some(parse_quote!(_: usize)),
            );
            let wrapper_args_into = crate::wrap_args(
                  &self.args,
                  |a| match a {
                        FnArg::Typed(PatType { pat, ty, .. }) => match *pat {
                              Pat::Ident(ident) => {
                                    let ret: Expr =
                                          parse_quote!(<#ty as solvent::SerdeReg>::decode(#ident));
                                    ret
                              }
                              _ => panic!("Function only receive typed args"),
                        },
                        _ => panic!("Function only receive typed args"),
                  },
                  None,
            );

            let wrapper: ItemFn = parse_quote! {
                  #[no_mangle]
                  extern "C" fn #wrapper_ident (#wrapper_args) -> usize {
                        let ret = #ident (#wrapper_args_into);
                        solvent::Error::encode(ret.map(|r| r.encode()))
                  }
            };
            wrapper.to_tokens(tokens);
      }
}
