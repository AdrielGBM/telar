//! `__baked_id!(kind, "id", path::to::telar)`: an id of a component-named asset kind, baked into the calling crate's artifact, as the value the kind's component accepts, `("id", Arc<Data>, monochrome)`.
//!
//! It is what a plugin's own macro expands to — `telar_icons::icon!("mdi:home")` — so Rust names a baked icon as surely as `icon name:"mdi:home"` does in `.rsx`. The data is inlined at the call site from the artifact rather than reached through `crate::__rsx_assets`, which only a crate wiring `.rsx` declares, so it works in a crate written wholly in Rust. An id the artifact does not hold is a `compile_error!` on the literal, naming what the bake reported.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{LitStr, Token};
use telar_project::{AssetContext, BakedId};

pub(crate) struct BakedIdInput {
    kind: Ident,
    id: LitStr,
    telar: TokenStream2,
}

impl Parse for BakedIdInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let kind = input.parse()?;
        input.parse::<Token![,]>()?;
        let id = input.parse()?;
        input.parse::<Token![,]>()?;
        let telar = input.parse()?;
        Ok(Self { kind, id, telar })
    }
}

type Stamp = Option<(SystemTime, u64)>;
type Loaded = (Stamp, Rc<AssetContext>);

thread_local! {
    // Keyed by the index's modification time and size as well as the package, since rust-analyzer's proc-macro server outlives a rebake.
    static CONTEXTS: RefCell<HashMap<PathBuf, Loaded>> = RefCell::new(HashMap::new());
}

fn context(manifest_dir: &Path) -> Rc<AssetContext> {
    let stamp = std::fs::metadata(
        manifest_dir
            .join(".telar")
            .join(telar_project::ASSETS_INDEX_FILENAME),
    )
    .and_then(|metadata| Ok((metadata.modified()?, metadata.len())))
    .ok();
    CONTEXTS.with(|contexts| {
        let mut contexts = contexts.borrow_mut();
        match contexts.get(manifest_dir) {
            Some((loaded, context)) if *loaded == stamp => Rc::clone(context),
            _ => {
                let context = Rc::new(AssetContext::load(
                    manifest_dir,
                    crate::ARTIFACT_TELAR_VERSION,
                ));
                contexts.insert(manifest_dir.to_path_buf(), (stamp, Rc::clone(&context)));
                context
            }
        }
    })
}

pub(crate) fn expand(input: BakedIdInput) -> syn::Result<TokenStream2> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map_err(|_| syn::Error::new(input.id.span(), "CARGO_MANIFEST_DIR not set"))?;
    expand_in(input, &manifest_dir)
}

fn expand_in(input: BakedIdInput, manifest_dir: &Path) -> syn::Result<TokenStream2> {
    let BakedIdInput { kind, id, telar } = input;
    let asset_kind = telar_project::asset_kind_for_id(&kind.to_string())
        .filter(|asset_kind| asset_kind.component.is_some())
        .ok_or_else(|| {
            syn::Error::new(
                kind.span(),
                format!("`{kind}` is not an asset kind a component names by id"),
            )
        })?;
    let context = context(manifest_dir);
    let BakedId {
        id: baked,
        monochrome,
        data_ty,
        init_expr,
    } = context
        .baked_id(asset_kind, &id.value())
        .map_err(|message| syn::Error::new(id.span(), message))?;
    let init: TokenStream2 = init_expr.parse().map_err(|e| {
        syn::Error::new(
            id.span(),
            format!("the baked artifact's initializer for `{baked}` does not parse: {e}. Run: cargo telar bake"),
        )
    })?;
    let data_ty = Ident::new(data_ty, Span::call_site());
    let tracked = context.index_file().map(|path| {
        let path = path.to_string_lossy().to_string();
        quote! { const _: &str = include_str!(#path); }
    });
    Ok(quote! {
        {
            #tracked
            #[allow(clippy::all)]
            static __TELAR_BAKED: ::std::sync::LazyLock<::std::sync::Arc<#telar::#data_ty>> =
                ::std::sync::LazyLock::new(|| {
                    #[allow(unused_imports)]
                    use #telar::*;
                    ::std::sync::Arc::new(#init)
                });
            (#baked, ::std::sync::Arc::clone(&*__TELAR_BAKED), #monochrome)
        }
    })
}

#[cfg(test)]
#[path = "baked_id_test.rs"]
mod tests;
