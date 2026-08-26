use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::HashMap;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, Fields, Ident, Token, punctuated::Punctuated};

/// The tokens whose built-in is a hard-coded constant, and therefore the ones a theme that stays silent
/// contradicts on screen: a component answering 4px next to bars the user configured to 10.
const REQUIRED: &[&str] = &[
    "primary",
    "on_primary",
    "radius",
    "spacing",
    "icon_size",
    "muted",
    "scrollbar",
    "ink",
    "surface",
    "surface_alt",
    "border",
    "success",
    "warning",
    "error",
    "info",
    "highlight_low",
    "highlight_med",
    "highlight_high",
];

/// `radius_sm`/`radius_md`/`radius_lg` derive from `radius`, so silence is the right answer rather than a
/// contradiction — a theme moves the base and the steps follow.
const DERIVED: &[&str] = &[
    "radius_sm",
    "radius_md",
    "radius_lg",
    "spacing_sm",
    "spacing_md",
    "spacing_lg",
    "spacing_xl",
];

/// Tokens a silent theme does not contradict, because their built-in adds nothing to the screen rather than
/// asserting a number beside one the theme chose. `root` is the theme's row at the top of the document: say
/// nothing and the document keeps its own, which is exactly right.
const OPTIONAL: &[&str] = &["root"];

fn is_token(name: &str) -> bool {
    REQUIRED.contains(&name) || DERIVED.contains(&name) || OPTIONAL.contains(&name)
}

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

/// Token → the field expression answering it, from same-named fields and `#[token(...)]` aliases.
fn parse_fields(input: &DeriveInput) -> syn::Result<HashMap<String, TokenStream2>> {
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

    let mut answered = HashMap::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named");
        let name = ident.to_string();
        if is_token(&name) {
            answered.insert(name, quote!(self.#ident));
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
                answered.insert(alias_name, quote!(self.#ident));
            }
        }
    }
    Ok(answered)
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
            None => match from_fields.get(token) {
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

    Ok(quote! {
        impl #impl_generics ::telar::ThemeTokens for #name #ty_generics #where_clause {
            #(#methods)*
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand_str(source: &str) -> syn::Result<String> {
        Ok(expand(syn::parse_str::<DeriveInput>(source)?)?.to_string())
    }

    /// Every token answered by a field of the same name, which is the shape the boilerplate had.
    const FULL: &str = "struct T {
        primary: Color, on_primary: Color, radius: f32, spacing: f32, font_size: f32,
        icon_size: f32, muted: Color, scrollbar: Color, ink: Color, surface: Color,
        surface_alt: Color, border: Color, success: Color, warning: Color, error: Color,
        info: Color, highlight_low: Color, highlight_med: Color, highlight_high: Color,
    }";

    #[test]
    fn same_named_fields_answer_their_tokens() {
        let out = expand_str(FULL).expect("every token is answered");
        assert!(out.contains("fn primary"), "emits the token methods");
    }

    /// The regression this derive exists for: a fixed built-in that contradicts the theme around it has to
    /// stop the build rather than reach the screen.
    #[test]
    fn an_unanswered_token_is_a_compile_error() {
        let err = expand_str(&FULL.replace("radius: f32,", "")).expect_err("radius has no answer");
        let message = err.to_string();
        assert!(message.contains("radius"), "names the token: {message}");
        assert!(
            message.contains("#[theme(default(radius))]"),
            "offers the deliberate opt-out: {message}"
        );
    }

    #[test]
    fn a_defaulted_token_is_accepted_and_left_to_the_trait() {
        let out = expand_str(&format!(
            "#[theme(default(radius))] {}",
            FULL.replace("radius: f32,", "")
        ))
        .expect("opting out answers it");
        assert!(
            !out.contains("fn radius ("),
            "no override, so the trait default stands"
        );
    }

    #[test]
    fn an_alias_answers_a_token_the_field_is_not_named_after() {
        let out = expand_str(&FULL.replace("error: Color,", "#[token(error)] danger: Color,"))
            .expect("the alias answers `error`");
        assert!(out.contains("self . danger"), "reads the aliased field");
    }

    #[test]
    fn an_expression_answers_a_token_no_field_holds() {
        let out = expand_str(&format!(
            "#[theme(scrollbar = self.muted.dim())] {}",
            FULL.replace("scrollbar: Color,", "")
        ))
        .expect("the expression answers `scrollbar`");
        assert!(out.contains("dim"), "emits the expression");
    }

    /// The three radius steps derive from `radius`, so silence there is the theme following its own base
    /// rather than a built-in contradicting it.
    #[test]
    fn the_radius_scale_is_not_required() {
        let out = expand_str(FULL).expect("valid");
        assert!(!out.contains("fn radius_md"), "left to derive from radius");
    }

    /// A metric step is an `f32`; answering one with the colour branch would not compile at the call site,
    /// which is a long way from the attribute that caused it.
    #[test]
    fn a_spacing_step_is_typed_as_a_metric() {
        let out = expand_str(&format!("#[theme(spacing_lg = 20.0)] {FULL}")).expect("valid");
        assert!(out.contains("fn spacing_lg (& self) -> f32"), "got: {out}");
    }

    #[test]
    fn a_token_that_does_not_exist_is_rejected() {
        let err = expand_str(&format!("#[theme(default(nonesuch))] {FULL}"))
            .expect_err("unknown token name");
        assert!(err.to_string().contains("not a ThemeTokens token"));
    }
}
