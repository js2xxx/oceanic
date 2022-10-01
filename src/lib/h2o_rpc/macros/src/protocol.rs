use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use quote::{ToTokens, __private::TokenStream as TokenStream2, quote};
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_quote,
    punctuated::Punctuated,
    Attribute, Error, Expr, FnArg, Ident, Path, Result, Signature, Token, Type, Visibility,
};

pub fn gen(args: TokenStream, input: Protocol) -> Result<TokenStream> {
    let event = parse_event(args)?;
    input.quote(event)
}

fn parse_event(input: TokenStream) -> Result<Path> {
    struct Wrapper(Path);
    impl Parse for Wrapper {
        fn parse(input: ParseStream) -> Result<Self> {
            let content;
            parenthesized!(content in input);
            Path::parse(&content).map(Wrapper)
        }
    }
    if input.is_empty() {
        Ok(parse_quote!(crate::UnknownEvent))
    } else {
        syn::parse::<Wrapper>(input).map(|w| w.0)
    }
}

pub struct Protocol {
    vis: Visibility,
    ident: Ident,
    doc: Vec<Attribute>,
    method: Punctuated<Method, Token![;]>,
}

impl Parse for Protocol {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attr = Attribute::parse_outer(input)?;
        attr.retain(|attr| attr.path.to_token_stream().to_string() == "doc");
        let vis = Visibility::parse(input)?;
        <Token![trait]>::parse(input)?;
        let ident = Ident::parse(input)?;
        let content;
        braced!(content in input);
        let method = Punctuated::parse_terminated(&content)?;
        Ok(Protocol {
            vis,
            ident,
            doc: attr,
            method,
        })
    }
}

pub struct Method {
    id: Expr,
    close: bool,
    ident: Ident,
    doc: Vec<Attribute>,
    const_ident: Ident,
    type_ident_prefix: String,
    args: Punctuated<FnArg, Token![,]>,
    output: Type,
}

impl Parse for Method {
    fn parse(input: ParseStream) -> Result<Self> {
        let meta = Attribute::parse_outer(input)?;

        let (id, close, doc) = {
            let mut id = None;
            let mut close = false;
            let mut doc = Vec::with_capacity(meta.len());

            for meta in meta {
                match &*meta.path.to_token_stream().to_string() {
                    "id" => id = Some(meta.parse_args()?),
                    "close" => {
                        if !meta.tokens.is_empty() {
                            return Err(Error::new_spanned(
                                meta.tokens,
                                "Invalid format for `#[close]`",
                            ));
                        }
                        close = true;
                    }
                    "doc" => doc.push(meta),
                    _ => {
                        let message = format!("Unsupported attribute {meta:?}");
                        return Err(Error::new_spanned(meta.tokens, message));
                    }
                }
            }

            (
                id.ok_or_else(|| Error::new(input.span(), "Provide a method id"))?,
                close,
                doc,
            )
        };
        let sig = Signature::parse(input)?;
        if let Some(ref c) = sig.constness {
            return Err(Error::new(c.span, "Protocol methods cannot be const"));
        }
        if sig.asyncness.is_none() {
            return Err(Error::new_spanned(
                quote!(#sig),
                "Protocol methods must be async",
            ));
        }
        if let Some(ref u) = sig.unsafety {
            return Err(Error::new(u.span, "Protocol methods cannot be unsafe"));
        }
        if let Some(ref r) = sig.generics.lt_token {
            return Err(Error::new(r.span, "Protocol methods cannot have generics"));
        }
        if let Some(ref v) = sig.variadic {
            return Err(Error::new(
                v.dots.spans[0],
                "Protocol methods cannot have varadic args",
            ));
        }

        let ident = sig.ident;
        let ident_str = ident.to_string();
        let const_ident = Ident::new(&ident_str.to_case(Case::UpperSnake), ident.span());
        let type_ident_prefix = ident_str.to_case(Case::UpperCamel);

        let args = sig.inputs;

        let output = match sig.output {
            syn::ReturnType::Default => parse_quote!(()),
            syn::ReturnType::Type(_, ty) => Box::into_inner(ty),
        };

        Ok(Method {
            id,
            close,
            ident,
            doc,
            const_ident,
            type_ident_prefix,
            args,
            output,
        })
    }
}

impl Method {
    fn constant(&self, vis: &Visibility) -> TokenStream2 {
        let Method {
            id, const_ident, ..
        } = self;
        quote!(#vis const #const_ident: usize = #id)
    }

    fn call_arg(&self) -> TokenStream2 {
        let iter = self.args.iter().map(|arg| match arg {
            FnArg::Typed(arg) => &*arg.pat,
            _ => unreachable!(),
        });
        quote!(#(#iter,)*)
    }

    fn call(&self) -> TokenStream2 {
        let Method {
            ident,
            doc,
            const_ident,
            args,
            output,
            ..
        } = self;
        let ser = self.call_arg();
        quote! {
            #(#doc)*
            pub async fn #ident (&self, #args) -> Result<#output, crate::Error> {
                let mut packet = Default::default();
                crate::packet::serialize(#const_ident, (#ser), &mut packet)?;
                let packet = self.inner.call(packet).await?;
                crate::packet::deserialize(#const_ident, &packet, None)
            }
        }
    }

    fn request(&self, prefix: &str) -> TokenStream2 {
        let Method {
            ident,
            doc,
            type_ident_prefix,
            args,
            ..
        } = self;
        let type_ident = Ident::new(type_ident_prefix, ident.span());
        let responder = self.responder_ident(prefix);
        quote! {
            #(#doc)*
            #type_ident {
                #args,
                responder: #responder
            }
        }
    }

    fn responder_ident(&self, prefix: &str) -> Ident {
        let ident = prefix.to_string() + &self.type_ident_prefix + "Responder";
        Ident::new(&ident, self.ident.span())
    }

    fn request_pat(&self, prefix: &str, req_ident: &Ident) -> TokenStream2 {
        let responder = self.responder_ident(prefix);
        let Method {
            ident,
            const_ident,
            type_ident_prefix,
            ..
        } = self;
        let type_ident = Ident::new(type_ident_prefix, ident.span());
        let pat = self.call_arg();
        quote! {
            #const_ident => {
                let (#pat) = crate::packet::deserialize_body(de, None)?;
                let responder = #responder {
                    inner: req.responder,
                };
                Ok(#req_ident:: #type_ident { #pat responder })
            }
        }
    }

    fn responder(&self, prefix: &str) -> TokenStream2 {
        let Method {
            const_ident,
            output,
            close,
            ..
        } = self;
        let ident = self.responder_ident(prefix);
        quote! {
            pub struct #ident {
                inner: crate::Responder,
            }

            impl #ident {
                pub fn send(self, ret: #output) -> Result<(), crate::Error> {
                    let mut packet = Default::default();
                    crate::packet::serialize(#const_ident, ret, &mut packet)?;
                    self.inner.send(packet, #close)
                }

                #[inline]
                pub fn close(self) {
                    self.inner.close()
                }
            }
        }
    }
}

impl Protocol {
    fn quote(self, event: Path) -> Result<TokenStream> {
        let Protocol {
            vis,
            ident,
            doc,
            method,
        } = self;

        let ident_str = ident.to_string();
        let std_mod = Ident::new(&(ident_str.to_case(Case::Snake) + "_std"), ident.span());
        let client = Ident::new(&(ident_str.clone() + "Client"), ident.span());
        let event_receiver = Ident::new(&(ident_str.clone() + "EventReceiver"), ident.span());
        let event_sender = Ident::new(&(ident_str.clone() + "EventSender"), ident.span());
        let request = Ident::new(&(ident_str.clone() + "Request"), ident.span());
        let server = Ident::new(&(ident_str.clone() + "Server"), ident.span());
        let stream = Ident::new(&(ident_str.clone() + "Stream"), ident.span());

        let constants = method.iter().map(|method| method.constant(&vis));
        let calls = method.iter().map(|method| method.call());
        let requests = method.iter().map(|method| method.request(&ident_str));
        let request_pats = method
            .iter()
            .map(|method| method.request_pat(&ident_str, &request));
        let responders = method.iter().map(|method| method.responder(&ident_str));

        let token = quote! {
            #(#constants;)*

            #[cfg(feature = "std")]
            mod #std_mod {
                use core::task::*;
                use core::pin::Pin;

                use futures::{Future, Stream, stream::FusedStream};
                use solvent_async::ipc::Channel;
                use solvent::ipc::Packet;

                use solvent_rpc::{SerdePacket, Event};
                use super::*;

                #[allow(dead_code)]
                fn assert_event() {
                    fn inner<T: crate::Event>() {}
                    inner::<#event>()
                }

                #(#doc)*
                #[derive(SerdePacket)]
                #vis struct #client {
                    inner: crate::Client,
                }

                impl #client {
                    pub fn new(channel: Channel) -> Self {
                        #client {
                            inner: crate::Client::new(channel),
                        }
                    }

                    #[inline]
                    fn from_inner(inner: crate::Client) -> Self {
                        #client { inner }
                    }

                    pub fn event_receiver(&self) -> Option<#event_receiver> {
                        self.inner
                            .event_receiver()
                            .map(|inner| #event_receiver { inner })
                    }

                    #(#calls)*
                }

                impl AsRef<Channel> for #client {
                    #[inline]
                    fn as_ref(&self) -> &Channel {
                        self.inner.as_ref()
                    }
                }

                impl From<Channel> for #client {
                    #[inline]
                    fn from(channel: Channel) -> Self {
                        Self::new(channel)
                    }
                }

                impl TryFrom<#client> for Channel {
                    type Error = #client;

                    #[inline]
                    fn try_from(client: #client) -> Result<Self, Self::Error> {
                        Channel::try_from(client.inner).map_err(|inner| #client { inner })
                    }
                }

                #vis struct #event_receiver {
                    inner: crate::EventReceiver,
                }

                impl Stream for #event_receiver {
                    type Item = Result<#event, crate::Error>;

                    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
                        Poll::Ready(
                            ready!(Pin::new(&mut self.inner).poll_next(cx))
                                .map(|inner| inner.and_then(<#event>::deserialize)),
                        )
                    }
                }

                impl FusedStream for #event_receiver {
                    fn is_terminated(&self) -> bool {
                        self.inner.is_terminated()
                    }
                }
                #[derive(SerdePacket)]
                pub struct #server {
                    inner: crate::Server,
                }

                impl #server {
                    pub fn new(channel: Channel) -> Self {
                        #server {
                            inner: crate::Server::new(channel),
                        }
                    }

                    #[inline]
                    fn from_inner(inner: crate::Server) -> Self {
                        #server { inner }
                    }

                    pub fn serve(self) -> (#stream, #event_sender) {
                        let (stream, es) = self.inner.serve();
                        (
                            #stream { inner: stream },
                            #event_sender { inner: es },
                        )
                    }
                }

                impl AsRef<Channel> for #server {
                    #[inline]
                    fn as_ref(&self) -> &Channel {
                        self.inner.as_ref()
                    }
                }

                impl From<Channel> for #server {
                    #[inline]
                    fn from(channel: Channel) -> Self {
                        Self::new(channel)
                    }
                }

                impl TryFrom<#server> for Channel {
                    type Error = #server;

                    #[inline]
                    fn try_from(server: #server) -> Result<Self, Self::Error> {
                        Channel::try_from(server.inner).map_err(|inner| #server { inner })
                    }
                }

                pub enum #request {
                    #(#requests,)*
                    Unknown(crate::Request),
                }

                pub struct #stream {
                    inner: crate::PacketStream,
                }

                impl Stream for #stream {
                    type Item = Result<#request, crate::Error>;

                    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
                        Poll::Ready(
                            ready!(Pin::new(&mut self.inner).poll_next(cx)).map(|res| match res {
                                Ok(req) => {
                                    let (m, de) = crate::packet::deserialize_metadata(&req.packet)?;
                                    match m {
                                        #(#request_pats)*
                                        _ => Ok(#request::Unknown(req)),
                                    }
                                }
                                Err(err) => Err(err),
                            }),
                        )
                    }
                }

                impl FusedStream for #stream {
                    #[inline]
                    fn is_terminated(&self) -> bool {
                        self.inner.is_terminated()
                    }
                }

                pub struct #event_sender {
                    inner: crate::EventSender,
                }

                impl #event_sender {
                    #[inline]
                    pub fn send_raw(&self, packet: Packet) -> Result<(), crate::Error> {
                        self.inner.send(packet)
                    }

                    #[inline]
                    pub fn close(self) {
                        self.inner.close()
                    }
                }

                #(#responders)*

                #(#doc)*
                pub fn with_disp(disp: solvent_async::disp::DispSender) -> (#client, #server) {
                    let (client, server) = crate::with_disp(disp);
                    (
                        <#client>::from_inner(client),
                        <#server>::from_inner(server),
                    )
                }

                #(#doc)*
                pub fn channel() -> (#client, #server) {
                    with_disp(solvent_async::dispatch())
                }
            }
            #[cfg(feature = "std")]
            pub use #std_mod::*;
        };
        Ok(token.into())
    }
}
