//! `telar::kw`, expanded from the one list of the shared vocabulary in `telar-project`: each word a name for its value, and each axis's type told its words.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, Path, Token, braced};
use telar_project::VOCABULARY;
use telar_project::naming::to_pascal_case;

/// The type each axis is a value of, in [`VOCABULARY`]'s order, with the variants of it no word names: `size => ControlSize, tone => Tone { Color }`.
pub struct Axes(Punctuated<Axis, Token![,]>);

struct Axis {
    name: Ident,
    ty: Path,
    unnamed: Vec<Ident>,
}

impl Parse for Axes {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Punctuated::parse_terminated(input).map(Self)
    }
}

impl Parse for Axis {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![=>]>()?;
        let ty = input.parse()?;
        let mut unnamed = Vec::new();
        if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            unnamed = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
        }
        Ok(Self { name, ty, unnamed })
    }
}

/// A name in the module for each word, the variant it writes, and an impl of the facade's `kw::Word` for each axis's type. Its matches are exhaustive both ways, so a word with no variant, or a variant with neither a word nor a place among the unnamed, fails to compile.
pub fn expand(axes: Axes) -> syn::Result<TokenStream2> {
    let written: Vec<String> = axes.0.iter().map(|axis| axis.name.to_string()).collect();
    let listed: Vec<&str> = VOCABULARY.iter().map(|axis| axis.name).collect();
    if written != listed {
        return Err(syn::Error::new(
            Span::call_site(),
            format!(
                "the vocabulary's axes are {}, in that order",
                listed.join(", ")
            ),
        ));
    }
    Ok(axes
        .0
        .iter()
        .zip(VOCABULARY)
        .map(|(axis, listed)| axis_items(axis, listed.words))
        .collect())
}

fn axis_items(axis: &Axis, words: &[&str]) -> TokenStream2 {
    let Axis { name, ty, unnamed } = axis;
    let name = name.to_string();
    let variants: Vec<Ident> = words
        .iter()
        .map(|word| Ident::new(&to_pascal_case(word), Span::call_site()))
        .collect();
    let markers: Vec<Ident> = words
        .iter()
        .map(|word| Ident::new(word, Span::call_site()))
        .collect();

    quote! {
        #(pub use #ty::#variants as #markers;)*

        impl crate::kw::Word for #ty {
            const AXIS: &'static str = #name;
            const WORDS: &'static [&'static str] = &[#(#words),*];

            fn word(&self) -> ::core::option::Option<&'static str> {
                match self {
                    #(Self::#variants => ::core::option::Option::Some(#words),)*
                    #(Self::#unnamed { .. } => ::core::option::Option::None,)*
                }
            }

            fn from_word(word: &str) -> ::core::option::Option<Self> {
                match word {
                    #(#words => ::core::option::Option::Some(Self::#variants),)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "vocabulary.test.rs"]
mod tests;
