//! The `t!("key", name = expr, ..)` translation macro.
//!
//! Validates the key (and its arguments) against the on-disk catalog at expansion time — an unknown key or a mismatched argument is a `compile_error!`, the build-time-safety payoff of the baked-catalog approach — then emits a runtime `telar::i18n::translate` call. The catalog is referenced by path (`crate::__rsx_i18n::CATALOG`) at the call site, never stored, so it always resolves to the current dylib under hot reload.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    Expr, LitStr, Token,
    parse::{Parse, ParseStream, Result as ParseResult},
};
use telar_transpiler::CatalogModel;

pub(crate) struct TInput {
    key: LitStr,
    args: Vec<(Ident, Expr)>,
}

impl Parse for TInput {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let key: LitStr = input.parse()?;
        let mut args = Vec::new();
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let name: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: Expr = input.parse()?;
            args.push((name, value));
        }
        Ok(TInput { key, args })
    }
}

thread_local! {
    // One build process expands every `t!` in the crate; cache the parsed catalog so hundreds of call sites don't each re-read the locale files. Keyed by package root; a fresh process (next build) starts empty.
    static CATALOG_CACHE: RefCell<HashMap<PathBuf, Rc<Result<Option<CatalogModel>, String>>>> =
        RefCell::new(HashMap::new());
}

fn load_catalog(manifest_dir: &Path) -> Rc<Result<Option<CatalogModel>, String>> {
    if let Some(hit) = CATALOG_CACHE.with(|c| c.borrow().get(manifest_dir).cloned()) {
        return hit;
    }
    let parsed = Rc::new(telar_transpiler::parse_catalog(manifest_dir));
    CATALOG_CACHE.with(|c| {
        c.borrow_mut()
            .insert(manifest_dir.to_path_buf(), parsed.clone())
    });
    parsed
}

pub(crate) fn expand(input: TInput) -> TokenStream2 {
    let TInput { key, args } = input;
    let key_str = key.value();

    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => return quote! { compile_error!("CARGO_MANIFEST_DIR not set") },
    };

    match &*load_catalog(&manifest_dir) {
        Ok(Some(model)) => {
            if !model.contains_key(&key_str) {
                let msg = format!("unknown i18n key `{key_str}`: not found in any catalog locale");
                return syn::Error::new(key.span(), msg).to_compile_error();
            }
            if let Some(expected) = model.arg_names(&key_str) {
                for (name, _) in &args {
                    let n = name.to_string();
                    if !expected.iter().any(|e| e == &n) {
                        let msg = format!("i18n key `{key_str}` has no placeholder `{{{n}}}`");
                        return syn::Error::new(name.span(), msg).to_compile_error();
                    }
                }
                for e in &expected {
                    if !args.iter().any(|(n, _)| &n.to_string() == e) {
                        let msg = format!(
                            "i18n key `{key_str}` is missing argument `{e}` (expected `{{{e}}}`)"
                        );
                        return syn::Error::new(key.span(), msg).to_compile_error();
                    }
                }
            }
        }
        Ok(None) => {
            let msg = format!(
                "`t!(\"{key_str}\")` used but no translation catalog exists — create `locales/<lang>.toml`"
            );
            return syn::Error::new(key.span(), msg).to_compile_error();
        }
        Err(msg) => return syn::Error::new(key.span(), msg.clone()).to_compile_error(),
    }

    let catalog_path: syn::Path =
        syn::parse_str(telar_transpiler::I18N_CATALOG_PATH).expect("catalog path is valid");

    let arg_lets: Vec<TokenStream2> = args
        .iter()
        .enumerate()
        .map(|(i, (_, value))| {
            let var = Ident::new(&format!("__rsx_t_arg_{i}"), Span::call_site());
            quote! { let #var = ::std::string::ToString::to_string(&(#value)); }
        })
        .collect();
    let arg_tuples: Vec<TokenStream2> = args
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            let var = Ident::new(&format!("__rsx_t_arg_{i}"), Span::call_site());
            let name_str = name.to_string();
            quote! { (#name_str, #var.as_str()) }
        })
        .collect();

    quote! {
        {
            #(#arg_lets)*
            ::telar::i18n::translate(&#catalog_path, #key_str, &[ #(#arg_tuples),* ])
        }
    }
}
