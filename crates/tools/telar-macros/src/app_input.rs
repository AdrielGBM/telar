use proc_macro2::{Ident, Span};
use telar_transpiler::naming::preview_entries_const_name;
use syn::{
    Token,
    parse::{Parse, ParseStream, Result as ParseResult},
};

pub(crate) struct AppInput {
    pub(crate) theme_type: syn::Path,
    pub(crate) setup: syn::Block,
    pub(crate) config: syn::Expr,
    pub(crate) app_expr: syn::Expr,
}

impl Parse for AppInput {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let theme_type = input.parse::<syn::Path>()?;
        input.parse::<Token![,]>()?;
        let setup = input.parse::<syn::Block>()?;
        input.parse::<Token![,]>()?;
        let config = input.parse::<syn::Expr>()?;
        input.parse::<Token![,]>()?;
        let app_expr = input.parse::<syn::Expr>()?;
        let _ = input.parse::<Token![,]>();
        Ok(AppInput {
            theme_type,
            setup,
            config,
            app_expr,
        })
    }
}

pub(crate) fn preview_const_ident(file_stem: &str) -> Ident {
    Ident::new(&preview_entries_const_name(file_stem), Span::call_site())
}
