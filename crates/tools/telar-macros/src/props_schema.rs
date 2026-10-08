//! The `PropsSchema` half of the `Props` derive: what the explorer shows of each prop, emitted only in a build with previews.

use proc_macro2::{Literal, TokenStream as TokenStream2};
use quote::{ToTokens, quote};
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, Expr, ExprLit, Ident, Lit, Meta, Token, Type, UnOp};

use crate::props::Prop;

/// `#[props(control = …)]`: how a prop's control differs from the one its type gets.
pub enum Control {
    /// Applied in order to the type's control, and to the control an `Option` wraps.
    Refine(Vec<TokenStream2>),
    /// Shown, never edited, whatever the type.
    ReadOnly,
}

const CONTROLS: &str = "the controls are `range(min, max)`, `step(step)`, `multiline` and `read_only`, and the first three chain: `range(0, 1).step(0.1)`";

impl Control {
    pub fn parse(expr: &Expr) -> Result<Self, syn::Error> {
        if let Expr::Path(path) = expr
            && path.path.is_ident("read_only")
        {
            return Ok(Self::ReadOnly);
        }
        let mut refinements = Vec::new();
        parse_chain(expr, &mut refinements)?;
        Ok(Self::Refine(refinements))
    }
}

fn parse_chain(expr: &Expr, out: &mut Vec<TokenStream2>) -> Result<(), syn::Error> {
    match expr {
        Expr::MethodCall(call) => {
            if let Some(turbofish) = &call.turbofish {
                return Err(syn::Error::new(turbofish.span(), CONTROLS));
            }
            parse_chain(&call.receiver, out)?;
            out.push(refinement(&call.method, Some(&call.args))?);
        }
        Expr::Call(call) => {
            let Expr::Path(path) = call.func.as_ref() else {
                return Err(syn::Error::new(call.func.span(), CONTROLS));
            };
            let Some(name) = path.path.get_ident() else {
                return Err(syn::Error::new(path.span(), CONTROLS));
            };
            out.push(refinement(name, Some(&call.args))?);
        }
        Expr::Path(path) => {
            let Some(name) = path.path.get_ident() else {
                return Err(syn::Error::new(path.span(), CONTROLS));
            };
            out.push(refinement(name, None)?);
        }
        other => return Err(syn::Error::new(other.span(), CONTROLS)),
    }
    Ok(())
}

/// One link of the chain, as the `ControlKind` method call it stands for.
fn refinement(
    name: &Ident,
    args: Option<&Punctuated<Expr, Token![,]>>,
) -> Result<TokenStream2, syn::Error> {
    let span = args.map_or(name.span(), Spanned::span);
    let numbers = |count: usize| -> Result<Vec<f64>, syn::Error> {
        let Some(args) = args.filter(|args| args.len() == count) else {
            return Err(syn::Error::new(
                name.span(),
                format!(
                    "`{name}` takes {count} number{}",
                    if count == 1 { "" } else { "s" }
                ),
            ));
        };
        args.iter().map(number).collect()
    };
    match name.to_string().as_str() {
        "range" => {
            let bounds = numbers(2)?;
            let (min, max) = (bounds[0], bounds[1]);
            if min > max {
                return Err(syn::Error::new(span, "a range's min is above its max"));
            }
            let (min, max) = (Literal::f64_suffixed(min), Literal::f64_suffixed(max));
            Ok(quote! { .range(#min, #max) })
        }
        "step" => {
            let step = numbers(1)?[0];
            if step <= 0.0 {
                return Err(syn::Error::new(span, "a step is above zero"));
            }
            let step = Literal::f64_suffixed(step);
            Ok(quote! { .step(#step) })
        }
        "multiline" if args.is_none_or(Punctuated::is_empty) => Ok(quote! { .multiline() }),
        "multiline" => Err(syn::Error::new(span, "`multiline` takes nothing")),
        "read_only" => Err(syn::Error::new(
            name.span(),
            "`read_only` replaces the control, so it stands alone",
        )),
        _ => Err(syn::Error::new(
            name.span(),
            format!("unknown control `{name}`: {CONTROLS}"),
        )),
    }
}

/// A number literal, negated or not: what a control's bounds are written as, checked here rather than when the explorer first opens.
fn number(expr: &Expr) -> Result<f64, syn::Error> {
    let parsed = match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(int), ..
        }) => int.base10_parse::<f64>(),
        Expr::Lit(ExprLit {
            lit: Lit::Float(float),
            ..
        }) => float.base10_parse::<f64>(),
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => {
            return number(&unary.expr).map(|value| -value);
        }
        other => return Err(syn::Error::new(other.span(), "expected a number literal")),
    };
    parsed.map_err(|error| syn::Error::new(expr.span(), error))
}

/// A doc text from `#[doc]` attributes: the literal lines unindented and joined, and any computed one (`#[doc = include_str!(…)]`) spliced in by `concat!`.
pub fn doc_of(attrs: &[Attribute]) -> TokenStream2 {
    enum Part {
        Lines(Vec<String>),
        Computed(Expr),
    }
    let mut parts: Vec<Part> = Vec::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("doc")) {
        let Meta::NameValue(pair) = &attr.meta else {
            continue;
        };
        match &pair.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) => {
                let lines = text
                    .value()
                    .split('\n')
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                match parts.last_mut() {
                    Some(Part::Lines(existing)) => existing.extend(lines),
                    _ => parts.push(Part::Lines(lines)),
                }
            }
            computed => parts.push(Part::Computed(computed.clone())),
        }
    }

    let written: Vec<&str> = parts
        .iter()
        .filter_map(|part| match part {
            Part::Lines(lines) => Some(lines),
            Part::Computed(_) => None,
        })
        .flatten()
        .map(String::as_str)
        .collect();
    let mut dedented = crate::text::dedent(&written).into_iter();
    let last = parts.len().saturating_sub(1);
    let pieces: Vec<TokenStream2> = parts
        .iter()
        .enumerate()
        .map(|(index, part)| match part {
            Part::Lines(lines) => {
                let text = dedented
                    .by_ref()
                    .take(lines.len())
                    .collect::<Vec<_>>()
                    .join("\n");
                let text = match (index == 0, index == last) {
                    (true, true) => text.trim(),
                    (true, false) => text.trim_start(),
                    (false, true) => text.trim_end(),
                    (false, false) => text.as_str(),
                };
                text.to_token_stream()
            }
            Part::Computed(expr) => expr.to_token_stream(),
        })
        .collect();
    match pieces.as_slice() {
        [] => quote! { "" },
        [only] => only.clone(),
        many => quote! { ::core::concat!(#(#many),*) },
    }
}

/// Emits `impl HasPropsSchema`, wrapped in `telar::__previews!` so a build without previews carries none of it.
pub fn expand(name: &Ident, doc: &TokenStream2, props: &[Prop]) -> TokenStream2 {
    let name_text = name.unraw().to_string();
    let fields = props.iter().map(field);
    quote! {
        ::telar::__previews! {
            impl ::telar::preview::HasPropsSchema for #name {
                fn schema() -> &'static ::telar::preview::PropsSchema {
                    static SCHEMA: ::telar::preview::PropsSchema =
                        ::telar::preview::PropsSchema::new(#name_text, #doc).fields(&[#(#fields),*]);
                    &SCHEMA
                }
            }
        }
    }
}

fn field(prop: &Prop) -> TokenStream2 {
    let name = prop.name.unraw().to_string();
    let ty = &prop.ty;
    let ty_text = type_text(ty);
    let doc = &prop.doc;
    let probed = quote! { ::telar::__preview_control!(#ty) };
    let control = match &prop.control {
        None => probed,
        Some(Control::ReadOnly) => quote! { ::telar::preview::ControlKind::ReadOnly },
        Some(Control::Refine(refinements)) => quote! {
            static INNER: ::std::sync::OnceLock<::telar::preview::ControlKind> = ::std::sync::OnceLock::new();
            ::telar::preview::__probe::refine(#probed, &INNER, |control| control #(#refinements)*)
        },
    };
    let default = match &prop.default {
        None => quote! {},
        Some(None) => {
            quote! { .default(::telar::preview::PropDefault::TypeDefault) }
        }
        Some(Some(expr)) => {
            let text = expr_text(expr);
            quote! { .default(::telar::preview::PropDefault::Expr(#text)) }
        }
    };
    let into = prop.into.then(|| quote! { .takes_into() });
    let some = prop.some.then(|| quote! { .takes_some() });
    quote! {
        ::telar::preview::PropField::new(#name, #ty_text, {
            fn control() -> ::telar::preview::ControlKind {
                #control
            }
            control
        })
        .doc(#doc)
        #default
        #into
        #some
    }
}

/// The type as rustfmt would write it, rather than the spaced-out token text: it is shown to whoever reads the docs.
pub fn type_text(ty: &Type) -> String {
    unparsed(quote! { type __Prop = #ty; }, "type __Prop = ")
        .unwrap_or_else(|| ty.to_token_stream().to_string())
}

pub fn expr_text(expr: &Expr) -> String {
    unparsed(quote! { const __PROP: () = #expr; }, "const __PROP: () = ")
        .unwrap_or_else(|| expr.to_token_stream().to_string())
}

fn unparsed(item: TokenStream2, prefix: &str) -> Option<String> {
    let file = syn::parse2::<syn::File>(item).ok()?;
    let text = prettyplease::unparse(&file);
    Some(
        text.trim_end()
            .strip_prefix(prefix)?
            .strip_suffix(';')?
            .to_owned(),
    )
}

#[cfg(test)]
#[path = "props_schema_test.rs"]
mod tests;
