//! The names `telar::preview::preview!` gives a Rust preview: its id and its component, both from the one tag, slugged here so `.rsx` and Rust previews share one id implementation.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token};

struct PreviewNamesInput {
    tag: Ident,
    name: LitStr,
}

impl Parse for PreviewNamesInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let tag = Ident::parse_any(input)?;
        input.parse::<Token![,]>()?;
        let name = input.parse()?;
        input.parse::<Option<Token![,]>>()?;
        Ok(Self { tag, name })
    }
}

/// Expands `tag, "Name"` into `(concat!(env!("CARGO_CRATE_NAME"), "--tag--name"), "tag")`: the `<crate>--<component>--<slug(name)>` id a generated `.rsx` entry has, and the component it names. A raw tag (`r#type`) is named without its prefix in both.
pub fn expand(input: TokenStream2) -> syn::Result<TokenStream2> {
    let PreviewNamesInput { tag, name } = syn::parse2(input)?;
    let component = tag.unraw().to_string();
    let suffix = telar_project::naming::preview_id_suffix(&component, &name.value())
        .map_err(|error| syn::Error::new(name.span(), error))?;
    Ok(quote! {
        (::core::concat!(::core::env!("CARGO_CRATE_NAME"), #suffix), #component)
    })
}

#[cfg(test)]
#[path = "preview_id_test.rs"]
mod tests;
