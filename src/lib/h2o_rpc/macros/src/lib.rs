use proc_macro::TokenStream;
use quote::{
    __private::{Span, TokenStream as TokenStream2},
    quote,
};
use syn::{
    punctuated::Punctuated, token::Comma, DeriveInput, Error, Fields, Ident, Result, Variant,
};

#[proc_macro_derive(SerdePacket)]
pub fn derive_serde_packet(input: TokenStream) -> TokenStream {
    match derive(input) {
        Ok(output) => output,
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive(input: TokenStream) -> Result<TokenStream> {
    let input = syn::parse::<DeriveInput>(input)?;
    Ok(match input.data {
        syn::Data::Struct(ref s) => derive_struct(&input.ident, &s.fields),
        syn::Data::Enum(ref e) => derive_enum(&input.ident, &e.variants),
        syn::Data::Union(_) => Err(Error::new_spanned(
            input,
            "`SerdePacket` doesn't support unions",
        ))?,
    })
}

fn derive_fields(name: &Ident, fields: &Fields) -> [TokenStream2; 3] {
    let pat = fields.iter().enumerate().map(|(index, field)| {
        if let Some(ref ident) = field.ident {
            quote!(#ident)
        } else {
            let ident = Ident::new(&format!("v{index}"), Span::call_site());
            quote!(#ident)
        }
    });
    let pat = match fields {
        Fields::Named(_) => quote!(#name { #(#pat),* }),
        Fields::Unnamed(_) => quote!(#name (#(#pat),*)),
        Fields::Unit => quote!(#name),
    };
    let ser = fields.iter().enumerate().map(|(index, field)| {
        if let Some(ref ident) = field.ident {
            quote!(SerdePacket::serialize(#ident, ser)?;)
        } else {
            let ident = Ident::new(&format!("v{index}"), Span::call_site());
            quote!(SerdePacket::serialize(#ident, ser)?;)
        }
    });
    let de = fields.iter().map(|field| {
        if let Some(ref ident) = field.ident {
            quote!(#ident: SerdePacket::deserialize(de)?,)
        } else {
            quote!(SerdePacket::deserialize(de)?,)
        }
    });
    let de = match &fields {
        syn::Fields::Named(_) => quote!(#name { #(#de)* }),
        syn::Fields::Unnamed(_) => quote!(#name (#(#de)*)),
        syn::Fields::Unit => quote!(#name),
    };
    [pat, quote!(#(#ser)*), de]
}

fn derive_struct(name: &Ident, fields: &Fields) -> TokenStream {
    let [pat, ser, de] = derive_fields(name, fields);
    quote! {
        impl solvent_rpc::packet::SerdePacket for #name {
            fn serialize(self, ser: &mut solvent_rpc::packet::Serializer)
                -> Result<(), solvent_rpc::Error>
            {
                #[allow(dead_code)]
                use solvent_rpc::packet::SerdePacket;
                let #pat = self;
                #ser
                Ok(())
            }

            fn deserialize(de: &mut solvent_rpc::packet::Deserializer)
                -> Result<Self, solvent_rpc::Error>
            {
                #[allow(dead_code)]
                use solvent_rpc::packet::SerdePacket;
                let ret = #de;
                Ok(ret)
            }
        }
    }
    .into()
}

fn derive_enum(name: &Ident, variants: &Punctuated<Variant, Comma>) -> TokenStream {
    let iter = variants.iter().enumerate().map(|(index, var)| {
        let ident = &var.ident;
        let fields = &var.fields;
        let [pat, ser, de] = derive_fields(ident, fields);

        let ser = quote!(#name ::#pat => { SerdePacket::serialize(#index, ser)?; #ser });
        let de = quote!(#index => #name ::#de,);
        (ser, de)
    });
    let (ser, de): (TokenStream2, TokenStream2) = iter.unzip();

    let len = variants.len();
    let token_stream = quote! {
        impl solvent_rpc::packet::SerdePacket for #name {
            fn serialize(self, ser: &mut solvent_rpc::packet::Serializer)
                -> Result<(), solvent_rpc::Error>
            {
                #[allow(dead_code)]
                use solvent_rpc::packet::SerdePacket;
                match self { #ser }
                Ok(())
            }

            fn deserialize(de: &mut solvent_rpc::packet::Deserializer)
                -> Result<Self, solvent_rpc::Error>
            {
                #[allow(dead_code)]
                use solvent_rpc::packet::SerdePacket;
                #[allow(dead_code)]
                use solvent_rpc::Error;
                let index: usize = SerdePacket::deserialize(de)?;
                let ret = match index {
                    #de
                    _ => return Err(Error::TypeMismatch(alloc::format!(
                        "unknown variant index {}, support 0..{}",
                        index,
                        #len
                    ).into()))
                };
                Ok(ret)
            }
        }
    };
    token_stream.into()
}
