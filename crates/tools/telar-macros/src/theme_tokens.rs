//! The `ThemeTokens` derive: forwarding a theme's fields to the catalogue's token trait, and registering the ones marked `#[theme(extension)]` as plugins' tokens.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::HashMap;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, Fields, Ident, Token, punctuated::Punctuated};
use telar_project::theme_tokens::{DERIVED, OPTIONAL, REQUIRED, is_token};

#[derive(Default)]
struct Options {
    /// Token → the expression answering it.
    values: HashMap<String, Expr>,
    /// Tokens the author accepted the built-in for, on purpose.
    defaulted: Vec<String>,
}

fn parse_struct_attrs(input: &DeriveInput) -> syn::Result<Options> {
    let mut options = Options::default();
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("theme")) {
        attr.parse_nested_meta(|meta| {
            let name = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            if name == "default" {
                let inner;
                syn::parenthesized!(inner in meta.input);
                for token in Punctuated::<Ident, Token![,]>::parse_terminated(&inner)? {
                    let token = token.to_string();
                    if !is_token(&token) {
                        return Err(meta.error(format!("`{token}` is not a ThemeTokens token")));
                    }
                    options.defaulted.push(token);
                }
                return Ok(());
            }
            if !is_token(&name) {
                return Err(meta.error(format!("`{name}` is not a ThemeTokens token")));
            }
            options.values.insert(name, meta.value()?.parse()?);
            Ok(())
        })?;
    }
    Ok(options)
}

#[derive(Default)]
struct FieldMap {
    /// Token → the field expression answering it, from same-named fields and `#[token(...)]` aliases.
    answered: HashMap<String, TokenStream2>,
    /// Fields marked `#[theme(extension)]`, each registered as its type's extension value.
    extensions: Vec<Ident>,
}

fn is_extension(attr: &syn::Attribute) -> syn::Result<bool> {
    let mut extension = false;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("extension") {
            extension = true;
            return Ok(());
        }
        Err(meta.error(
            "a field takes `#[theme(extension)]`; a token is answered by a field's name or `#[token(...)]`",
        ))
    })?;
    Ok(extension)
}

fn parse_fields(input: &DeriveInput) -> syn::Result<FieldMap> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "ThemeTokens can only be derived for a struct",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            input.span(),
            "ThemeTokens needs named fields to map tokens by name",
        ));
    };

    let mut map = FieldMap::default();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named");
        let name = ident.to_string();
        if is_token(&name) {
            map.answered.insert(name, quote!(self.#ident));
        }
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("theme")) {
            if is_extension(attr)? {
                map.extensions.push(ident.clone());
            }
        }
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("token")) {
            for alias in attr.parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)? {
                let alias_name = alias.to_string();
                if !is_token(&alias_name) {
                    return Err(syn::Error::new(
                        alias.span(),
                        format!("`{alias_name}` is not a ThemeTokens token"),
                    ));
                }
                map.answered.insert(alias_name, quote!(self.#ident));
            }
        }
    }
    Ok(map)
}

fn return_type(token: &str) -> TokenStream2 {
    match token {
        "root" => quote!(::telar::Declared),
        t if t.starts_with("radius") || t.starts_with("spacing") || t == "icon_size" => {
            quote!(f32)
        }
        _ => quote!(::telar::Color),
    }
}

/// Expands the `ThemeTokens` derive, forwarding a theme's fields to the catalogue's token trait.
pub fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let options = parse_struct_attrs(&input)?;
    let from_fields = parse_fields(&input)?;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut methods = Vec::new();
    let mut missing = Vec::new();

    for token in REQUIRED.iter().chain(DERIVED).chain(OPTIONAL) {
        let token = *token;
        if options.defaulted.iter().any(|d| d == token) {
            continue;
        }
        let body = match options.values.get(token) {
            Some(expr) => quote!(#expr),
            None => match from_fields.answered.get(token) {
                Some(field) => field.clone(),
                None => {
                    if REQUIRED.contains(&token) {
                        missing.push(token);
                    }
                    continue;
                }
            },
        };
        let method = Ident::new(token, input.ident.span());
        let ty = return_type(token);
        methods.push(quote! {
            fn #method(&self) -> #ty {
                #body
            }
        });
    }

    if !missing.is_empty() {
        let list = missing.join("`, `");
        return Err(syn::Error::new(
            input.ident.span(),
            format!(
                "nothing answers the `{list}` token(s), and their built-ins are fixed values that will \
                 contradict this theme on screen. Answer each one with a field of that name, an existing \
                 field marked `#[token({first})]`, a value via `#[theme({first} = ...)]`, or accept the \
                 built-in on purpose with `#[theme(default({first}))]`.",
                first = missing[0]
            ),
        ));
    }

    if !from_fields.extensions.is_empty() {
        let fields = &from_fields.extensions;
        methods.push(quote! {
            fn register_extensions(&self, extensions: &mut ::telar::ThemeExtensions) {
                #(extensions.insert(::core::clone::Clone::clone(&self.#fields));)*
            }
        });
    }

    Ok(quote! {
        impl #impl_generics ::telar::ThemeTokens for #name #ty_generics #where_clause {
            #(#methods)*
        }
    })
}

#[cfg(test)]
#[path = "theme_tokens_test.rs"]
mod tests;
