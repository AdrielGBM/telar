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
    /// What a field holding a `Handler<fn(…)>`, or with `some` an `Option` of one, is called with: its setter takes a closure of those arguments.
    pub callback: Option<Callback>,
}

/// A callback prop's handler, and the arguments of the closure its setter takes.
pub struct Callback {
    /// The `Handler<…>` type as the field writes it.
    handler: Type,
    arguments: Vec<Type>,
    /// Whether the field holds an `Option` of the handler.
    optional: bool,
}

impl Callback {
    /// The function type the handler is named by, `fn(A, B)`, which [`IntoHandler`](telar::IntoHandler) is keyed on.
    fn signature(&self) -> TokenStream2 {
        let arguments = &self.arguments;
        quote! { fn(#(#arguments),*) }
    }
}

impl Prop {
    fn is_required(&self) -> bool {
        self.default.is_none()
    }

    /// The setter's parameter type and the expression that stores it.
    ///
    /// **`impl Into<T>` is opt-in, and that is not timidity.** A generic parameter leaves a literal's type unconstrained, so it falls back — `.size(20.0)` infers `f64` and needs `f32: From<f64>`, which does not exist (a warning today, a hard error soon), and `.span(5)` infers `i32` and needs `u32: From<i32>`, which is an error already. Coercion is right for a prop whose type exists to accept several shapes, `Reactive<T>` above all; it is wrong for a plain number. Declaring it means forgetting it fails loudly on the author's line instead of quietly bending a literal.
    ///
    /// **`some` and `into` are separate knobs because they answer separate questions.** `Option<Reactive< String>>` wants both — the caller writes `hint:"⌘Z"` and means `Some(Reactive::from(…))`. But `Option<Rc<dyn Fn() -> bool>>` wants only `some`: a shared closure reaches its `dyn` type by unsizing, which is a coercion at a known parameter type and not an `Into` a generic can find. Folding them into one attribute makes one of those two cases impossible to express.
    ///
    /// **A callback's setter is bounded by `Fn` itself**, since that bound is the only one rustc reads a closure's argument types from: through an `Into<Handler<fn(bool)>>`, `.on_toggle(|on| …)` would need `|on: bool|`.
    fn setter_param(&self) -> (TokenStream2, TokenStream2) {
        let ty = &self.ty;
        if let Some(callback) = &self.callback {
            let (handler, arguments) = (&callback.handler, &callback.arguments);
            let held = quote! { <#handler as ::core::convert::From<_>>::from(value) };
            return (
                quote! { impl ::core::ops::Fn(#(#arguments),*) + 'static },
                match callback.optional {
                    true => quote! { ::core::option::Option::Some(#held) },
                    false => held,
                },
            );
        }
        let inner = self.some.then(|| option_inner(ty)).flatten();
        let (param_ty, value) = match &inner {
            Some(inner) => (inner, quote! { ::core::option::Option::Some(value) }),
            None => (ty, quote! { value }),
        };
        if let Some(read) = reactive_inner(param_ty) {
            return (
                quote! { impl ::telar::IntoReading<#read> },
                match inner.is_some() {
                    true => quote! {
                        ::core::option::Option::Some(::telar::IntoReading::into_reading(value))
                    },
                    false => quote! { ::telar::IntoReading::into_reading(value) },
                },
            );
        }
        match self.into {
            true => (
                quote! { impl ::core::convert::Into<#param_ty> },
                match inner.is_some() {
                    true => quote! { ::core::option::Option::Some(value.into()) },
                    false => quote! { value.into() },
                },
            ),
            false if is_number(param_ty) => (
                quote! { impl ::telar::PropNumber<#param_ty> },
                match inner.is_some() {
                    true => quote! {
                        ::core::option::Option::Some(::telar::PropNumber::into_prop(value))
                    },
                    false => quote! { ::telar::PropNumber::into_prop(value) },
                },
            ),
            false => (quote! { #param_ty }, value),
        }
    }

    /// `maybe_<name>`, on a callback prop only: what a handler held already is handed on with, as an `Option`, the way a component hands its own callback prop to one it calls. `None` leaves the prop as it was, which is why a callback prop always has a default.
    fn maybe_setter(&self) -> TokenStream2 {
        let Some(callback) = &self.callback else {
            return quote! {};
        };
        let (name, handler) = (&self.name, &callback.handler);
        let maybe = format_ident!("maybe_{}", name.unraw());
        let held = match callback.optional {
            true => quote! { ::core::option::Option::Some(value) },
            false => quote! { value },
        };
        let doc = format!(
            "[`{name}`](Self::{name}) for a handler held already, such as a component's own callback prop handed on: `None` leaves the prop as it was."
        );
        quote! {
            #[doc = #doc]
            pub fn #maybe(mut self, value: ::core::option::Option<#handler>) -> Self {
                if let ::core::option::Option::Some(value) = value {
                    self.#name = #held;
                }
                self
            }
        }
    }

    /// `__forward_<name>`: what a `.rsx` call hands a value it names to — a binding, a `props` place, a path — since the markup cannot say what the name holds. The prop's own setter takes it, except on a callback prop, whose setter takes only a closure: there it may also be a [`Handler`](telar::Handler) or an `Option` of one ([`IntoHandler`](telar::IntoHandler)), handed to `maybe_<name>`. `returns` is what the prop's own setter returns.
    fn forward_setter(&self, returns: &TokenStream2) -> TokenStream2 {
        let name = &self.name;
        let forward = format_ident!("__forward_{}", name.unraw());
        let Some(callback) = &self.callback else {
            let (param, _) = self.setter_param();
            return quote! {
                #[doc(hidden)]
                pub fn #forward(self, value: #param) -> #returns {
                    self.#name(value)
                }
            };
        };
        let signature = callback.signature();
        let maybe = format_ident!("maybe_{}", name.unraw());
        quote! {
            #[doc(hidden)]
            pub fn #forward(self, value: impl ::telar::IntoHandler<#signature>) -> #returns {
                self.#maybe(::telar::IntoHandler::into_handler(value))
            }
        }
    }

    /// `__follow_<name>`: what a `.rsx` call hands a worked-out value to, as a closure, so the prop's type decides what becomes of it. A `Reactive<T>` prop follows what the value read ([`Reactive::follow`](telar::Reactive::follow)), the value being anything that reads as a `T` ([`IntoReading`](telar::IntoReading)), as a colour does as a `Tone`; any other takes the value worked out once, a callback prop anything `__forward_<name>` takes, and panics if working it out read state, since it would never hear that state change: the one case only the run can see. `returns` is what the prop's own setter returns.
    fn follow_setter(&self, props: &Ident, returns: &TokenStream2) -> TokenStream2 {
        let name = &self.name;
        let follow = format_ident!("__follow_{}", name.unraw());
        let target = match self.some {
            true => option_inner(&self.ty).unwrap_or_else(|| self.ty.clone()),
            false => self.ty.clone(),
        };
        if let Some(inner) = reactive_inner(&target) {
            return quote! {
                #[doc(hidden)]
                pub fn #follow<V: ::telar::IntoReading<#inner>>(
                    self,
                    read: impl Fn() -> V + 'static,
                ) -> #returns {
                    self.#name(::telar::Reactive::<#inner>::follow(read))
                }
            };
        }
        let setter = match self.callback {
            Some(_) => format_ident!("__forward_{}", name.unraw()),
            None => name.clone(),
        };
        let (generics, value) = match (&self.callback, self.into, is_number(&target)) {
            (Some(callback), _, _) => {
                let signature = callback.signature();
                (
                    quote! { <V: ::telar::IntoHandler<#signature>> },
                    quote! { V },
                )
            }
            (None, true, _) => (quote! { <V: ::core::convert::Into<#target>> }, quote! { V }),
            (None, false, true) => (quote! { <V: ::telar::PropNumber<#target>> }, quote! { V }),
            (None, false, false) => (quote! {}, quote! { #target }),
        };
        let remedy = match self.callback {
            Some(_) => {
                "read it with `untrack(…)` to take it once, or hand a closure that reads it when it runs"
            }
            None => {
                "declare it `Reactive<…>` to follow what it reads, or read it with `untrack(…)` to take it once"
            }
        };
        let message = format!(
            "`{}` of `{props}` takes a value worked out once, and the one it was handed read state, which it would never hear change: {remedy}",
            name.unraw()
        );
        quote! {
            #[doc(hidden)]
            #[track_caller]
            pub fn #follow #generics(self, read: impl FnOnce() -> #value) -> #returns {
                let (value, read_state) = ::telar::reads_state(read);
                if read_state {
                    ::core::panic!(#message);
                }
                self.#setter(value)
            }
        }
    }
}

/// What a `Handler<fn(A, B)>` is called with, `[A, B]`, and nothing for a bare `Handler`, written by any path that ends in it; `None` for any other type. A handler named by anything but its `fn(…)` is refused, since the closure its setter takes is spelled from those arguments.
fn handler_arguments(ty: &Type) -> Result<Option<Vec<Type>>, syn::Error> {
    let Type::Path(path) = ty else {
        return Ok(None);
    };
    let Some(last) = path.path.segments.last() else {
        return Ok(None);
    };
    if path.qself.is_some() || last.ident != "Handler" {
        return Ok(None);
    }
    let signature = match &last.arguments {
        syn::PathArguments::None => return Ok(Some(Vec::new())),
        syn::PathArguments::AngleBracketed(args) => {
            match args.args.iter().collect::<Vec<_>>()[..] {
                [syn::GenericArgument::Type(Type::FnPtr(signature))]
                    if matches!(signature.output, syn::ReturnType::Default) =>
                {
                    Some(signature)
                }
                _ => None,
            }
        }
        syn::PathArguments::Parenthesized(_) => None,
    };
    match signature {
        Some(signature) => Ok(Some(
            signature.inputs.iter().map(|arg| arg.ty.clone()).collect(),
        )),
        None => Err(syn::Error::new(
            last.arguments.span(),
            "a callback prop names what its handler is called with as `Handler<fn(…)>`, since its setter takes a closure of those arguments",
        )),
    }
}

/// The callback a field holds: a `Handler<fn(…)>`, or with `some` an `Option` of one. Refuses the declarations its setters cannot serve: `into`, which the closure setter leaves nothing to convert; an `Option` without `some`, whose setter would take the `Option` rather than the closure; and no default, since a `None` handed to `maybe_<name>` leaves the prop as it was and a required prop has nothing to be.
fn callback_of(
    name: &Ident,
    ty: &Type,
    some: bool,
    into: bool,
    defaulted: bool,
) -> Result<Option<Callback>, syn::Error> {
    let key = name.unraw();
    if !some
        && let Some(inner) = option_inner(ty)
        && handler_arguments(&inner)?.is_some()
    {
        return Err(syn::Error::new(
            ty.span(),
            format!(
                "`{key}` holds an optional callback, which is `#[props(some, default)]`: its setter takes the closure, and `maybe_{key}` the `Option` of a handler held already"
            ),
        ));
    }
    let held = some.then(|| option_inner(ty)).flatten();
    let optional = held.is_some();
    let handler = held.unwrap_or_else(|| ty.clone());
    let Some(arguments) = handler_arguments(&handler)? else {
        return Ok(None);
    };
    if into {
        return Err(syn::Error::new(
            name.span(),
            format!(
                "`{key}` is a callback, whose setter takes a closure and whose `maybe_{key}` takes a handler held already: drop `into`, which leaves nothing to convert"
            ),
        ));
    }
    if !defaulted {
        return Err(syn::Error::new(
            name.span(),
            format!(
                "`{key}` is a callback, which needs a default — `#[props(some, default)]` on an `Option<Handler<…>>`, or `#[props(default = Handler::from(…))]` — since a `None` handed on through `maybe_{key}` leaves the prop as it was"
            ),
        ));
    }
    Ok(Some(Callback {
        handler,
        arguments,
        optional,
    }))
}

/// `T` when `ty` is a `Reactive<T>`, written by any path that ends in it.
fn reactive_inner(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else { return None };
    let last = path.path.segments.last()?;
    if last.ident != "Reactive" {
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

/// Whether `ty` is a primitive number, whose setter also takes the [`Int`](telar::Int) or [`Float`](telar::Float) a `.rsx` literal is handed over as. Only these: a Rust literal reaching the setter still has the one integer or float type to be.
fn is_number(ty: &Type) -> bool {
    const NUMBERS: [&str; 12] = [
        "i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize", "f32", "f64",
    ];
    match ty {
        Type::Path(path) => {
            path.qself.is_none()
                && path
                    .path
                    .get_ident()
                    .is_some_and(|ident| NUMBERS.iter().any(|n| ident == n))
        }
        _ => false,
    }
}

pub fn expand(input: DeriveInput) -> Result<TokenStream2, syn::Error> {
    let props = collect(&input)?;
    let in_flow = !draws_on_a_layer(&input.attrs)?;
    let (name, vis) = (&input.ident, &input.vis);
    let builder = format_ident!("{name}Builder");
    let declared: Vec<TokenStream2> = input
        .generics
        .type_params()
        .map(|param| quote! { #param })
        .collect();
    let named: Vec<TokenStream2> = input
        .generics
        .type_params()
        .map(|param| {
            let ident = &param.ident;
            quote! { #ident }
        })
        .collect();
    let where_clause = &input.generics.where_clause;
    let own = angled(named.iter().cloned());
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
    let with_own = |declare: bool, rest: Vec<TokenStream2>| {
        let lead = match declare {
            true => &declared,
            false => &named,
        };
        angled(lead.iter().cloned().chain(rest))
    };

    let slot_list: Vec<TokenStream2> = slots.iter().map(|s| quote! { #s }).collect();
    let declared_generics = with_own(true, slot_list.clone());
    let generics = with_own(false, slot_list);
    let all_missing = with_own(false, markers.iter().map(|m| quote! { #m }).collect());
    let all_set = with_own(
        false,
        required
            .iter()
            .map(|p| {
                let ty = &p.ty;
                quote! { #ty }
            })
            .collect(),
    );
    let own_declared = with_own(true, Vec::new());

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
        let maybe = p.maybe_setter();
        let forward = p.forward_setter(&quote! { Self });
        let follow = p.follow_setter(&input.ident, &quote! { Self });
        quote! {
            pub fn #name(mut self, value: #param) -> Self {
                self.#name = #stored;
                self
            }
            #maybe
            #forward
            #follow
        }
    });

    // A required prop's setter moves one parameter from the marker to the real type and leaves the others generic, so the chain may be written in any order — and calling it twice finds no method, because the second call would need a builder whose slot is still `Missing`.
    let required_setters = required.iter().map(|target| {
        let (name, ty) = (&target.name, &target.ty);
        let others: Vec<TokenStream2> = slots
            .iter()
            .enumerate()
            .filter(|(i, _)| required[*i].name != target.name)
            .map(|(_, s)| quote! { #s })
            .collect();
        let render = |set: bool| {
            with_own(
                false,
                required
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let (slot, marker) = (&slots[i], &markers[i]);
                        match (p.name == target.name, set) {
                            (true, true) => quote! { #ty },
                            (true, false) => quote! { #marker },
                            (false, _) => quote! { #slot },
                        }
                    })
                    .collect(),
            )
        };
        let (before, after) = (render(false), render(true));
        let (param, stored) = target.setter_param();
        let forward = target.forward_setter(&quote! { #builder #after });
        let follow = target.follow_setter(&input.ident, &quote! { #builder #after });
        let carried = props.iter().map(|p| {
            let field = &p.name;
            match p.name == target.name {
                true => quote! { #field: #stored },
                false => quote! { #field: self.#field },
            }
        });
        let others = with_own(true, others);
        quote! {
            impl #others #builder #before #where_clause {
                pub fn #name(self, value: #param) -> #builder #after {
                    #builder { #(#carried),* }
                }
                #forward
                #follow
            }
        }
    });

    let moved = props.iter().map(|p| {
        let field = &p.name;
        quote! { #field: self.#field }
    });

    // A `Clone` of its own rather than a derive the author writes, because a props struct that cannot be cloned cannot reach a closure that runs again, and `[view]` puts one around any node whose layout is computed. This is also why a callback prop is a shared `Handler`: a unique box has no second owner to give.
    let cloned = props.iter().map(|p| {
        let field = &p.name;
        quote! { #field: ::core::clone::Clone::clone(&self.#field) }
    });

    // A call's placement check names the props type without its parameters, which nothing at the check infers: the constant stands on the one instance with every parameter `()`, so the path resolves to it.
    let in_flow_on = angled(named.iter().map(|_| quote! { () }));

    let schema = props_schema::expand(
        name,
        &input.generics,
        &props_schema::doc_of(&input.attrs),
        &props,
    );
    let actions = preview_actions(
        name,
        &builder,
        PreviewGenerics {
            own_declared: &own_declared,
            own: &own,
            declared: &declared_generics,
            named: &generics,
            where_clause,
        },
        &props,
    );

    Ok(quote! {
        #(
            /// Stands in for a required prop that has not been set, and names it in the error when `build` turns out not to exist.
            #[doc(hidden)]
            #vis struct #markers;
        )*

        #vis struct #builder #declared_generics #where_clause {
            #(#fields),*
        }

        impl #name #in_flow_on {
            /// Whether the item the component returns stands where its call does, so a call may place it (`width:`, `grow:`, `self:`): false for one drawn on a layer of its own.
            #[doc(hidden)]
            pub const IN_FLOW: bool = #in_flow;
        }

        impl #own_declared #name #own #where_clause {
            /// Starts a props builder. Every optional prop already holds its default; a required one holds the `Missing` marker, and `build` does not exist until none are left.
            pub fn props() -> #builder #all_missing {
                #builder { #(#seeded),* }
            }
        }

        impl #declared_generics #builder #generics #where_clause {
            #(#optional_setters)*
        }

        #(#required_setters)*

        impl #own_declared #builder #all_set #where_clause {
            pub fn build(self) -> #name #own {
                #name { #(#moved),* }
            }
        }

        impl #own_declared ::core::clone::Clone for #name #own #where_clause {
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

/// The parameter lists the preview actions are written over: the struct's own, declared and named, and the builder's, which add the slots.
struct PreviewGenerics<'a> {
    own_declared: &'a TokenStream2,
    own: &'a TokenStream2,
    declared: &'a TokenStream2,
    named: &'a TokenStream2,
    where_clause: &'a Option<syn::WhereClause>,
}

/// Emits, wrapped in `telar::__previews!`, the builder's `__preview_actions` over the optional props, callable in any builder state, and the props' `PreviewActions` over every prop.
///
/// Every prop goes through the action probe rather than only those whose type is written as a `Fn`, so a callback reached through a type alias is logged too, and a prop that is no callback comes back as it was.
fn preview_actions(
    name: &Ident,
    builder: &Ident,
    generics: PreviewGenerics<'_>,
    props: &[Prop],
) -> TokenStream2 {
    let PreviewGenerics {
        own_declared,
        own,
        declared,
        named,
        where_clause,
    } = generics;
    let optional: Vec<&Prop> = props.iter().filter(|p| !p.is_required()).collect();
    let all: Vec<&Prop> = props.iter().collect();
    let builder_fn = logging_fn(quote! { pub fn __preview_actions }, &optional);
    let props_fn = logging_fn(quote! { fn preview_actions }, &all);
    quote! {
        ::telar::__previews! {
            impl #declared #builder #named #where_clause {
                #[doc(hidden)]
                #builder_fn
            }

            impl #own_declared ::telar::preview::PreviewActions for #name #own #where_clause {
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

    for param in &input.generics.params {
        match param {
            syn::GenericParam::Type(param) if param.default.is_some() => {
                return Err(syn::Error::new(
                    param.span(),
                    "a props struct's type parameter takes no default: a call infers it from the props it is handed",
                ));
            }
            syn::GenericParam::Type(_) => {}
            _ => {
                return Err(syn::Error::new(
                    param.span(),
                    "a props struct takes type parameters only: a lifetime or a constant has no place in props a closure may run again with",
                ));
            }
        }
    }

    named.named.iter().map(read_prop).collect()
}

/// Reads a field and its `#[props(…)]`: `default`, `default = expr`, `into`, `some`, `doc = "…"` and `control = …`, in any combination and across any number of attributes. No attribute at all means a required prop whose setter takes its type exactly.
fn read_prop(field: &syn::Field) -> Result<Prop, syn::Error> {
    let name = field.ident.clone().expect("named fields have idents");
    let key = name.unraw().to_string();
    if let Some(does) = telar_project::call_keys::kept_by_the_call(&key) {
        return Err(syn::Error::new(
            name.span(),
            format!(
                "a component takes no prop named `{key}`: `{key}:` on a call {does}, so it never reaches a prop; name this one for what it means to the component"
            ),
        ));
    }
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
    let callback = callback_of(&name, &field.ty, some, into, default.is_some())?;
    Ok(Prop {
        name,
        ty: field.ty.clone(),
        default,
        into,
        some,
        doc: doc.unwrap_or_else(|| props_schema::doc_of(&field.attrs)),
        control,
        callback,
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

/// Whether the struct says `#[props(layer)]`: its component draws on a layer of its own, a dialog or a notice, not where its call stands, so a call has nothing to place.
fn draws_on_a_layer(attrs: &[syn::Attribute]) -> Result<bool, syn::Error> {
    let mut layer = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("props")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("layer") {
                layer = true;
                Ok(())
            } else {
                Err(meta.error("the props struct's own attribute is `layer`"))
            }
        })?;
    }
    Ok(layer)
}
