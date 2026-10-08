//! The `PreviewArg` derive: a fieldless enum as a choice control, one option per variant.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields};

/// Expands the derive into an impl of `telar::preview::PreviewArg`, gated with `telar::__previews!` so the enum compiles the same in a build without previews.
pub fn expand(input: DeriveInput) -> Result<TokenStream2, syn::Error> {
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new(
            input.ident.span(),
            "`PreviewArg` derives only for an enum whose variants have no fields",
        ));
    };
    if data.variants.is_empty() {
        return Err(syn::Error::new(
            input.ident.span(),
            "`PreviewArg` needs at least one variant to choose",
        ));
    }
    let mut idents = Vec::with_capacity(data.variants.len());
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new(
                variant.fields.span(),
                "`PreviewArg` derives only for variants with no fields",
            ));
        }
        if variant.ident == "none" {
            return Err(syn::Error::new(
                variant.ident.span(),
                "`none` is how an arg's text form writes an unset value, so a variant cannot be named it",
            ));
        }
        idents.push(&variant.ident);
    }
    let names: Vec<String> = idents
        .iter()
        .map(|ident| ident.unraw().to_string())
        .collect();
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        ::telar::__previews! {
            impl #impl_generics ::telar::preview::PreviewArg for #name #ty_generics #where_clause {
                const CONTROL: ::telar::preview::ControlKind = ::telar::preview::ControlKind::Choice {
                    variants: &[#(#names),*],
                };

                fn to_arg_value(&self) -> ::telar::preview::ArgValue {
                    let name = match self {
                        #(Self::#idents => #names,)*
                    };
                    ::telar::preview::ArgValue::Choice(::std::string::String::from(name))
                }

                fn from_arg_value(value: &::telar::preview::ArgValue) -> ::core::option::Option<Self> {
                    match value {
                        ::telar::preview::ArgValue::Choice(name) => match name.as_str() {
                            #(#names => ::core::option::Option::Some(Self::#idents),)*
                            _ => ::core::option::Option::None,
                        },
                        _ => ::core::option::Option::None,
                    }
                }
            }
        }
    })
}

#[cfg(test)]
#[path = "preview_arg_test.rs"]
mod tests;
