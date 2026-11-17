//! The `ComponentTokens` derive: reading a plugin's own tokens from the theme in force, and answering each one from the field or from a declared fallback.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, Field, Fields, GenericArgument, PathArguments, Type};

struct Fallback {
    expr: Expr,
    scaled: bool,
}

fn parse_fallback(field: &Field) -> syn::Result<Option<Fallback>> {
    let mut expr = None;
    let mut scaled = false;
    let mut declared = false;
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("token")) {
        declared = true;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("or") {
                expr = Some(meta.value()?.parse::<Expr>()?);
            } else if meta.path.is_ident("scaled") {
                scaled = true;
            } else {
                return Err(meta.error("a token takes `or = fallback` and `scaled`"));
            }
            Ok(())
        })?;
    }
    match (expr, scaled) {
        (Some(expr), scaled) => Ok(Some(Fallback { expr, scaled })),
        (None, true) => Err(syn::Error::new(
            field.span(),
            "`scaled` scales a set value and calls the fallback with the size: it needs `or = fallback`",
        )),
        (None, false) if declared => Err(syn::Error::new(
            field.span(),
            "`#[token(...)]` needs `or = fallback`; a field answered by hand has no attribute",
        )),
        (None, false) => Ok(None),
    }
}

fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

fn getter(field: &Field) -> syn::Result<Option<TokenStream2>> {
    let Some(fallback) = parse_fallback(field)? else {
        return Ok(None);
    };
    let name = field.ident.as_ref().expect("named fields are checked");
    let Some(held) = option_inner(&field.ty) else {
        return Err(syn::Error::new(
            field.ty.span(),
            "a field answered by a fallback is an `Option`: `None` is where the fallback applies",
        ));
    };
    let docs = field.attrs.iter().filter(|a| a.path().is_ident("doc"));
    let Fallback { expr, scaled } = fallback;
    Ok(Some(if scaled {
        quote! {
            #(#docs)*
            pub fn #name(&self, size: ::telar::ControlSize) -> #held {
                let fallback: fn(::telar::ControlSize) -> #held = #expr;
                self.#name.map_or_else(|| fallback(size), |set| set * size.scale())
            }
        }
    } else {
        quote! {
            #(#docs)*
            pub fn #name(&self) -> #held {
                self.#name.unwrap_or_else(#expr)
            }
        }
    }))
}

pub fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "ComponentTokens derives on a struct",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            input.span(),
            "ComponentTokens derives on a struct of named fields",
        ));
    };
    let getters = fields
        .named
        .iter()
        .map(getter)
        .collect::<syn::Result<Vec<_>>>()?;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// The tokens of the theme in force, subscribing the caller to it.
            pub fn current() -> Self {
                ::telar::use_theme_extension::<Self>()
            }

            #(#getters)*
        }
    })
}

#[cfg(test)]
#[path = "component_tokens.test.rs"]
mod tests;
