extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::visit_mut::VisitMut;
use syn::*;

struct ModifyIdent {
      val: LitInt,
      repl: Ident,
}

impl VisitMut for ModifyIdent {
      fn visit_expr_mut(&mut self, node: &mut Expr) {
            if let Expr::Path(ExprPath { path, .. }) = node.to_owned() {
                  if path.is_ident(&self.repl) {
                        let val = self.val.to_owned();
                        *node = parse_quote!(#val);
                  }
            }

            visit_mut::visit_expr_mut(self, node);
      }
}

struct RepeatExpr {
      var: Ident,
      start: LitInt,
      end: LitInt,
      body: __private::TokenStream2,
      prefix: LitStr,
      punc: LitStr,
      suffix: LitStr,
}

impl syn::parse::Parse for RepeatExpr {
      fn parse(input: parse::ParseStream) -> Result<Self> {
            let prefix = if input.peek(LitStr) {
                  input.parse()?
            } else {
                  LitStr::new("", __private::Span::call_site())
            };
            input.parse::<Token![for]>()?;
            let var = input.parse()?;
            input.parse::<Token![in]>()?;
            let start = input.parse()?;
            input.parse::<Token![..]>()?;
            let end = input.parse()?;
            let body;
            braced!(body in input);
            let punc = if input.peek(LitStr) {
                  input.parse()?
            } else {
                  LitStr::new("", __private::Span::call_site())
            };
            let suffix = if input.peek(LitStr) {
                  input.parse()?
            } else {
                  LitStr::new("", __private::Span::call_site())
            };
            Ok(RepeatExpr {
                  var,
                  start,
                  end,
                  body: body.parse()?,
                  prefix,
                  punc,
                  suffix,
            })
      }
}

impl ToTokens for RepeatExpr {
      fn to_tokens(&self, tokens: &mut __private::TokenStream2) {
            let start: usize = self
                  .start
                  .base10_parse()
                  .expect("Failed to parse repeat start");
            let end: usize = self.end.base10_parse().expect("Failed to parse repeat end");
            let var_str = {
                  let mut s = String::from("# ");
                  s.push_str(&self.var.to_string());
                  s
            };

            let mut out = String::new();
            out.push_str(&self.prefix.value());

            for i in start..end {
                  let body_input = self.body.to_string();
                  let mut body_output = String::new();

                  for word in body_input.split_inclusive(&var_str) {
                        if word.ends_with(&var_str) {
                              body_output.push_str(word.split_at(word.len() - var_str.len()).0);
                              body_output.push_str(&i.to_string());
                        } else {
                              body_output.push_str(word);
                        }
                  }
                  let body_input = body_output;
                  let mut body_output = String::new();

                  for (i, o) in body_input.split("[<").enumerate() {
                        if i == 0 {
                              body_output.push_str(o);
                        } else {
                              let (words, o) =
                                    o.split_once(">]").expect("'[<' and '>]' should be paired");
                              for word in words.split_ascii_whitespace() {
                                    body_output.push_str(word);
                              }
                              body_output.push_str(o);
                        }
                  }

                  out.push_str(&body_output);
                  out.push_str(&self.punc.value());
            }

            out.push_str(&self.suffix.value());

            out.parse::<__private::TokenStream2>()
                  .expect("Failed to parse output stream")
                  .to_tokens(tokens);
      }
}

#[proc_macro]
pub fn repeat(input: TokenStream) -> TokenStream {
      let expr = parse_macro_input!(input as RepeatExpr);
      quote!(#expr).into()
}
