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
use telar_transpiler::CatalogContext;

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
    // One build process expands every `t!` in the crate; cache the index so hundreds of call sites don't each re-read it. Keyed by package root; a fresh process (next build) starts empty.
    static CATALOG_CACHE: RefCell<HashMap<PathBuf, Rc<CatalogContext>>> =
        RefCell::new(HashMap::new());
}

fn load_catalog(manifest_dir: &Path) -> Rc<CatalogContext> {
    if let Some(hit) = CATALOG_CACHE.with(|c| c.borrow().get(manifest_dir).cloned()) {
        return hit;
    }
    // This crate's own version, for the same reason the asset artifact compares against it: what matters is the `telar` whose API the generated module calls, and that is whoever loads it.
    let loaded = Rc::new(CatalogContext::load(
        manifest_dir,
        env!("CARGO_PKG_VERSION"),
    ));
    CATALOG_CACHE.with(|c| {
        c.borrow_mut()
            .insert(manifest_dir.to_path_buf(), loaded.clone())
    });
    loaded
}

pub(crate) fn expand(input: TInput) -> TokenStream2 {
    let TInput { key, args } = input;
    let key_str = key.value();

    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => return quote! { compile_error!("CARGO_MANIFEST_DIR not set") },
    };

    let catalog = load_catalog(&manifest_dir);
    match catalog.arg_names(&key_str) {
        Ok(Some(expected)) => {
            for (name, _) in &args {
                let n = name.to_string();
                if !expected.iter().any(|e| e == &n) {
                    let msg = format!("i18n key `{key_str}` has no placeholder `{{{n}}}`");
                    return syn::Error::new(name.span(), msg).to_compile_error();
                }
            }
            for e in expected {
                if !args.iter().any(|(n, _)| &n.to_string() == e) {
                    let msg = format!(
                        "i18n key `{key_str}` is missing argument `{e}` (expected `{{{e}}}`)"
                    );
                    return syn::Error::new(key.span(), msg).to_compile_error();
                }
            }
        }
        // An artifact that exists and defines nothing is a project with no translations; one that defines other keys but not this one is a typo. The two deserve different advice, and only the index can tell them apart — which is why it is written even when there is nothing to write.
        Ok(None) if catalog.is_empty() => {
            let msg = format!(
                "`t!(\"{key_str}\")` used but no translation catalog exists — create `locales/<lang>.toml`"
            );
            return syn::Error::new(key.span(), msg).to_compile_error();
        }
        Ok(None) => {
            let msg = format!("unknown i18n key `{key_str}`: not found in any catalog locale");
            return syn::Error::new(key.span(), msg).to_compile_error();
        }
        Err(msg) => return syn::Error::new(key.span(), msg).to_compile_error(),
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
