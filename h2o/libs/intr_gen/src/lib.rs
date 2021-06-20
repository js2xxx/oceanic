extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::*;

/// Generate a interrupt handler and a declaration of its routine 
/// (defined in `stubs.asm`) for H2O.
///
/// NOTE: This macro must be used in pairs with [`rout`].
#[proc_macro_attribute]
pub fn hdl(args: TokenStream, func: TokenStream) -> TokenStream {
      assert!(
            args.to_string().is_empty(),
            "No attribute args accepted. Please setup in the entry."
      );
      let mut func = parse_macro_input!(func as ItemFn);

      let rout_ident = format_ident!("rout_{}", func.sig.ident);

      assert!(
            func.sig.constness.is_none() && func.sig.asyncness.is_none(),
            "The interrupt handler must not be `const` or `async`."
      );
      func.sig.abi = Some(parse_quote!(extern "C"));
      func.sig.ident = format_ident!("hdl_{}", func.sig.ident);
      assert!(
            func.sig.generics.params.iter().next().is_none(),
            "{}\n{}",
            "The interrupt handler must be without generic parameters",
            "\tor a lifetime other than `'static`."
      );

      let msg =
            "The interrupt handler only accepts one `*mut Frame` \n\tor `*const Frame` parameter.";
      for (i, arg) in func.sig.inputs.iter().enumerate() {
            assert!(i < 1, "{}", msg);
            assert!(
                  !matches!(arg, FnArg::Typed(PatType { ty, .. })
            if ty.to_token_stream().to_string() == "*mut Frame"
                  || ty.to_token_stream().to_string() == "*const Frame"),
                  "{}",
                  msg
            );
      }

      assert!(
            matches!(func.sig.output, ReturnType::Default),
            "The interrupt handler must return nothing."
      );
      quote!(
            extern "C" {
                  fn #rout_ident();
            }
            #[no_mangle]
            #func
      )
      .into()
}

/// Generate the name of the interrupt routine. Used for initialization.
///
/// NOTE: This macro must be used in pairs with [`hdl`].
#[proc_macro]
pub fn rout(input: TokenStream) -> TokenStream {
      let ident = parse_macro_input!(input as Ident);
      let ident = format_ident!("rout_{}", ident);
      quote!(#ident).into()
}

