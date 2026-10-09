//! The `Props` derive: a typed builder whose `build()` exists only once every required prop is set.

use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, format_ident, quote};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, ExprLit, Fields, Ident, Lit, Type};

use crate::props_schema::{self, Control};

/// One prop: whether omitting it is legal and whether its setter coerces, for the builder, and its doc and control, for the schema.
pub struct Prop {
    pub name: Ident,
    pub ty: Type,
    /// `None` for a required prop. `Some(None)` for `#[props(default)]`, `Some(Some(expr))` for `#[props(default = expr)]`.
    pub default: Option<Option<Expr>>,
    /// `#[props(into)]`: the setter takes `impl Into<T>` rather than `T`.
    pub into: bool,
    /// `#[props(some)]`: the field is `Option<Inner>` and the setter takes the `Inner`, wrapping it. What makes "this prop was given" the callee's business rather than the caller's.
    pub some: bool,
    /// `#[props(doc = …)]`, or else the field's doc comment.
    pub doc: TokenStream2,
    pub control: Option<Control>,
}

impl Prop {
    fn is_required(&self) -> bool {
        self.default.is_none()
    }

    /// The setter's parameter type and the expression that stores it.
    ///
    /// **`impl Into<T>` is opt-in, and that is not timidity.** A generic parameter leaves a literal's type unconstrained, so it falls back — `.size(20.0)` infers `f64` and needs `f32: From<f64>`, which does not exist (a warning today, a hard error soon), and `.span(5)` infers `i32` and needs `u32: From<i32>`, which is an error already. Coercion is right for a prop whose type exists to accept several shapes, `Reactive<T>` above all; it is wrong for a plain number. Declaring it means forgetting it fails loudly on the author's line instead of quietly bending a literal.
    ///
    /// **`some` and `into` are separate knobs because they answer separate questions.** `Option<Reactive< String>>` wants both — the caller writes `hint:"⌘Z"` and means `Some(Reactive::from(…))`. But `Option<Box<dyn Fn(u32)>>` wants only `some`: a boxed closure reaches its `dyn` type by unsizing, which is a coercion at a known parameter type and not an `Into` a generic can find. Folding them into one attribute makes one of those two cases impossible to express.
    fn setter_param(&self) -> (TokenStream2, TokenStream2) {
        let ty = &self.ty;
        let inner = self.some.then(|| option_inner(ty)).flatten();
        let (param_ty, value) = match &inner {
            Some(inner) => (inner, quote! { ::core::option::Option::Some(value) }),
            None => (ty, quote! { value }),
        };
        match self.into {
            true => (
                quote! { impl ::core::convert::Into<#param_ty> },
                match inner.is_some() {
                    true => quote! { ::core::option::Option::Some(value.into()) },
                    false => quote! { value.into() },
                },
            ),
            false => (quote! { #param_ty }, value),
        }
    }
}

/// Expands the `Props` derive into the builder, its markers and the `build` that needs them all set.
pub fn expand(input: DeriveInput) -> Result<TokenStream2, syn::Error> {
    let props = collect(&input)?;
    let (name, vis) = (&input.ident, &input.vis);
    let builder = format_ident!("{name}Builder");
    let required: Vec<&Prop> = props.iter().filter(|p| p.is_required()).collect();
    // One marker per required prop, generated here rather than imported from a runtime crate, which every deriving crate would then have to name. Per prop rather than one shared marker because the name lands in the error, so `RowPropsBuilder<RowPropsMissingLabel>` says which prop was forgotten. Named after the struct too, because two props structs in one module are ordinary and a marker named only after the prop collided.
    let markers: Vec<Ident> = required
        .iter()
        .map(|p| format_ident!("{name}Missing{}", pascal(&p.name)))
        .collect();
    let marker_of = |prop: &Prop| {
        required
            .iter()
            .position(|r| r.name == prop.name)
            .map(|i| markers[i].clone())
    };
    // One type parameter per required prop, standing in for that prop's type once it is set. The marker is the field's value until then, so `build` — which exists only where every parameter is the real type — reads the fields straight out, with nothing to unwrap.
    let slots: Vec<Ident> = (0..required.len()).map(|i| format_ident!("S{i}")).collect();
    let slot_of = |prop: &Prop| {
        required
            .iter()
            .position(|r| r.name == prop.name)
            .map(|i| slots[i].clone())
    };

    let generics = angled(slots.iter().map(|s| quote! { #s }));
    let all_missing = angled(markers.iter().map(|m| quote! { #m }));
    let all_set = angled(required.iter().map(|p| {
        let ty = &p.ty;
        quote! { #ty }
    }));

    let fields = props.iter().map(|p| {
        let name = &p.name;
        match slot_of(p) {
            Some(slot) => quote! { #name: #slot },
            None => {
                let ty = &p.ty;
                quote! { #name: #ty }
            }
        }
    });

    let seeded = props.iter().map(|p| {
        let name = &p.name;
        let marker = marker_of(p);
        let value = match &p.default {
            None => quote! { #marker },
            Some(None) => {
                let ty = &p.ty;
                quote! { <#ty as ::core::default::Default>::default() }
            }
            Some(Some(expr)) => quote! { #expr },
        };
        quote! { #name: #value }
    });

    // An optional prop keeps its type whatever the slots are, so its setter takes `self` and gives it back.
    let optional_setters = props.iter().filter(|p| !p.is_required()).map(|p| {
        let name = &p.name;
        let (param, stored) = p.setter_param();
        quote! {
            pub fn #name(mut self, value: #param) -> Self {
                self.#name = #stored;
                self
            }
        }
    });

    // A required prop's setter moves one parameter from the marker to the real type and leaves the others generic, so the chain may be written in any order — and calling it twice finds no method, because the second call would need a builder whose slot is still `Missing`.
    let required_setters = required.iter().map(|target| {
        let (name, ty) = (&target.name, &target.ty);
        let others: Vec<&Ident> = slots
            .iter()
            .enumerate()
            .filter(|(i, _)| required[*i].name != target.name)
            .map(|(_, s)| s)
            .collect();
        let render = |set: bool| {
            angled(required.iter().enumerate().map(|(i, p)| {
                let (slot, marker) = (&slots[i], &markers[i]);
                match (p.name == target.name, set) {
                    (true, true) => quote! { #ty },
                    (true, false) => quote! { #marker },
                    (false, _) => quote! { #slot },
                }
            }))
        };
        let (before, after) = (render(false), render(true));
        let (param, stored) = target.setter_param();
        let carried = props.iter().map(|p| {
            let field = &p.name;
            match p.name == target.name {
                true => quote! { #field: #stored },
                false => quote! { #field: self.#field },
            }
        });
        let others = angled(others.iter().map(|s| quote! { #s }));
        quote! {
            impl #others #builder #before {
                pub fn #name(self, value: #param) -> #builder #after {
                    #builder { #(#carried),* }
                }
            }
        }
    });

    let moved = props.iter().map(|p| {
        let field = &p.name;
        quote! { #field: self.#field }
    });

    // A `Clone` of its own rather than a derive the author writes, because a props struct that cannot be cloned cannot reach a closure that runs again, and `[view]` puts one around any node whose layout is computed. This is also why a handler prop is an `Rc<dyn Fn…>`: a unique box has no second owner to give.
    let cloned = props.iter().map(|p| {
        let field = &p.name;
        quote! { #field: ::core::clone::Clone::clone(&self.#field) }
    });

    let schema = props_schema::expand(name, &props_schema::doc_of(&input.attrs), &props);
    let actions = preview_actions(name, &builder, &generics, &props);

    Ok(quote! {
        #(
            /// Stands in for a required prop that has not been set, and names it in the error when `build` turns out not to exist.
            #[doc(hidden)]
            #vis struct #markers;
        )*

        #vis struct #builder #generics {
            #(#fields),*
        }

        impl #name {
            /// Starts a props builder. Every optional prop already holds its default; a required one holds the `Missing` marker, and `build` does not exist until none are left.
            pub fn props() -> #builder #all_missing {
                #builder { #(#seeded),* }
            }
        }

        impl #generics #builder #generics {
            #(#optional_setters)*
        }

        #(#required_setters)*

        impl #builder #all_set {
            pub fn build(self) -> #name {
                #name { #(#moved),* }
            }
        }

        impl ::core::clone::Clone for #name {
            fn clone(&self) -> Self {
                #name { #(#cloned),* }
            }
        }

        #schema

        #actions
    })
}

/// `<A, B>`, or nothing at all when the list is empty — `Foo<>` is not valid Rust, and a props struct whose every field has a default produces exactly that.
fn angled(args: impl Iterator<Item = TokenStream2>) -> TokenStream2 {
    let args: Vec<TokenStream2> = args.collect();
    match args.is_empty() {
        true => quote! {},
        false => quote! { <#(#args),*> },
    }
}

/// Emits, wrapped in `telar::__previews!`, the builder's `__preview_actions` over the optional props, callable in any builder state, and the props' `PreviewActions` over every prop.
///
/// Every prop goes through the action probe rather than only those whose type is written as a `Fn`, so a callback reached through a type alias is logged too, and a prop that is no callback comes back as it was.
fn preview_actions(
    name: &Ident,
    builder: &Ident,
    generics: &TokenStream2,
    props: &[Prop],
) -> TokenStream2 {
    let optional: Vec<&Prop> = props.iter().filter(|p| !p.is_required()).collect();
    let all: Vec<&Prop> = props.iter().collect();
    let builder_fn = logging_fn(quote! { pub fn __preview_actions }, &optional);
    let props_fn = logging_fn(quote! { fn preview_actions }, &all);
    quote! {
        ::telar::__previews! {
            impl #generics #builder #generics {
                #[doc(hidden)]
                #builder_fn
            }

            impl ::telar::preview::PreviewActions for #name {
                #props_fn
            }
        }
    }
}

/// `fn(self, log: &ActionLog) -> Self` passing each of `props` through the action probe, with no `mut` and an unused `_log` when there are none, so the expansion raises no warning in the deriving crate.
fn logging_fn(signature: TokenStream2, props: &[&Prop]) -> TokenStream2 {
    if props.is_empty() {
        return quote! {
            #signature(self, _log: &::telar::preview::ActionLog) -> Self {
                self
            }
        };
    }
    let logged = props.iter().map(|p| {
        let (field, text) = (&p.name, p.name.unraw().to_string());
        quote! { self.#field = ::telar::__preview_action!(*log, #text, self.#field); }
    });
    quote! {
        #signature(mut self, log: &::telar::preview::ActionLog) -> Self {
            #(#logged)*
            self
        }
    }
}

fn collect(input: &DeriveInput) -> Result<Vec<Prop>, syn::Error> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "Props describes a struct of named fields",
        ));
    };
    let Fields::Named(named) = &data.fields else {
        return Err(syn::Error::new(
            data.fields.span(),
            "Props needs named fields: a builder setter is named after the prop it sets",
        ));
    };

    // The builder's slots and markers name the struct without its parameters, and a static schema cannot depend on them either.
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(
            input.generics.span(),
            "a props struct takes no generic parameters: a tag names one props type, and its builder and schema are written for that one type",
        ));
    }

    named.named.iter().map(read_prop).collect()
}

/// Reads a field and its `#[props(…)]`: `default`, `default = expr`, `into`, `some`, `doc = "…"` and `control = …`, in any combination and across any number of attributes. No attribute at all means a required prop whose setter takes its type exactly.
fn read_prop(field: &syn::Field) -> Result<Prop, syn::Error> {
    let (mut default, mut into, mut some, mut doc, mut control) = (None, false, false, None, None);
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("props")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("into") {
                into = true;
            } else if meta.path.is_ident("some") {
                some = true;
            } else if meta.path.is_ident("default") {
                default = Some(match meta.input.peek(syn::Token![=]) {
                    true => Some(meta.value()?.parse::<Expr>()?),
                    false => None,
                });
            } else if meta.path.is_ident("doc") {
                doc = Some(match meta.value()?.parse::<Expr>()? {
                    Expr::Lit(ExprLit {
                        lit: Lit::Str(text),
                        ..
                    }) => text.value().trim().to_token_stream(),
                    computed => computed.to_token_stream(),
                });
            } else if meta.path.is_ident("control") {
                control = Some(Control::parse(&meta.value()?.parse::<Expr>()?)?);
            } else {
                return Err(meta.error(
                    "the prop attributes are `default`, `into`, `some`, `doc` and `control`",
                ));
            }
            Ok(())
        })?;
    }
    Ok(Prop {
        name: field.ident.clone().expect("named fields have idents"),
        ty: field.ty.clone(),
        default,
        into,
        some,
        doc: doc.unwrap_or_else(|| props_schema::doc_of(&field.attrs)),
        control,
    })
}

/// `on_press` -> `OnPress`, so a marker type reads as a type rather than as a field name.
fn pascal(name: &Ident) -> String {
    name.to_string()
        .split('_')
        .map(|part| match part.chars().next() {
            Some(head) => head.to_ascii_uppercase().to_string() + &part[head.len_utf8()..],
            None => String::new(),
        })
        .collect()
}

/// The `T` of an `Option<T>`, or `None` when the type is not one.
fn option_inner(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else { return None };
    let last = path.path.segments.last()?;
    if last.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(inner) => Some(inner.clone()),
        _ => None,
    })
}
