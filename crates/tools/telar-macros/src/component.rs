//! `#[component]`: a Rust function with named arguments, read as a tag.

use proc_macro2::TokenStream as TokenStream2;
use quote::format_ident;
use quote::quote;
use syn::spanned::Spanned;
use syn::{FnArg, Ident, ItemFn, Pat, Result, parse2};

/// Turns a function of named arguments into the pair a `[view]` calls: a props struct and a component.
///
/// A widget that markup cannot build itself — one that owns a canvas, a register, a document — reaches a `[view]` as a component with named props, which is the shape the child position takes since the `widget` escape went. Written by hand that is a struct, a `derive`, a destructuring `let` and a signature nobody reads; the arguments already say all four.
pub fn expand(item: TokenStream2) -> Result<TokenStream2> {
    let function: ItemFn = parse2(item)?;
    let signature = &function.sig;
    for (what, span) in [
        signature.asyncness.map(|a| ("async", a.span())),
        signature.constness.map(|c| ("const", c.span())),
    ]
    .into_iter()
    .flatten()
    {
        return Err(syn::Error::new(
            span,
            format!("a component cannot be `{what}`: a `[view]` calls it as it builds"),
        ));
    }
    if !signature.generics.params.is_empty() {
        return Err(syn::Error::new(
            signature.generics.span(),
            "a component takes no type parameters: a tag names one props type, and rustc has nothing to infer them from",
        ));
    }

    let mut fields = Vec::new();
    let mut names = Vec::new();
    let mut children = None;
    for arg in &signature.inputs {
        let FnArg::Typed(arg) = arg else {
            return Err(syn::Error::new(
                arg.span(),
                "a component is a free function, not a method",
            ));
        };
        let Pat::Ident(name) = arg.pat.as_ref() else {
            return Err(syn::Error::new(
                arg.pat.span(),
                "every argument of a component is a prop, so each one needs a name",
            ));
        };
        let (name, ty, attrs) = (&name.ident, &arg.ty, &arg.attrs);
        // The one argument that is not a prop: what the call site nested inside the tag, which every component takes whether or not it uses them.
        if name == "children" {
            children = Some(quote! { let #name: #ty = __children; });
            continue;
        }
        names.push(name.clone());
        fields.push(quote! { #(#attrs)* pub #name: #ty });
    }

    let props_name = props_type_name(&signature.ident);
    let (vis, name, output, body) = (
        &function.vis,
        &signature.ident,
        &signature.output,
        &function.block,
    );
    let docs: Vec<_> = function
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .collect();
    let kept: Vec<_> = function
        .attrs
        .iter()
        .filter(|a| !a.path().is_ident("doc"))
        .collect();
    let children = children.unwrap_or_else(|| quote! { let _ = __children; });

    Ok(quote! {
        #(#docs)*
        #[derive(::telar::Props)]
        #vis struct #props_name {
            #(#fields,)*
        }

        #(#docs)*
        #(#kept)*
        #vis fn #name(
            __props: #props_name,
            __children: ::telar::Children,
        ) #output {
            let #props_name { #(#names,)* } = __props;
            #children
            #body
        }
    })
}

/// The props type a tag of this name asks for, spelled the way the transpiler spells it: `graph_canvas` is `GraphCanvasProps`. A tag names its props type, so the two have to agree letter for letter.
fn props_type_name(name: &Ident) -> Ident {
    let pascal: String = name
        .to_string()
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut letters = part.chars();
            match letters.next() {
                Some(first) => first.to_uppercase().chain(letters).collect::<String>(),
                None => String::new(),
            }
        })
        .collect();
    format_ident!("{}Props", pascal, span = name.span())
}
