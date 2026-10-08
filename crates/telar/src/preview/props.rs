//! The props metadata a preview's docs and controls are generated from, written by `#[derive(Props)]` through [`HasPropsSchema`].

use super::ControlKind;

/// A props struct the workshop can describe. `#[derive(Props)]` implements it in a build with previews, so naming it is how a preview points at its component's props: `.props(<ButtonProps as HasPropsSchema>::schema)`.
pub trait HasPropsSchema {
    fn schema() -> &'static PropsSchema;
}

/// A component's props, as its docs and controls see them.
///
/// Built with [`PropsSchema::new`] and the `const` setters, so a generated schema stays a `static` and a field added later is not a breaking change.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct PropsSchema {
    /// The props type's name.
    pub name: &'static str,
    /// The props type's doc comment.
    pub doc: &'static str,
    /// In declaration order.
    pub fields: &'static [PropField],
}

impl PropsSchema {
    pub const fn new(name: &'static str, doc: &'static str) -> Self {
        Self {
            name,
            doc,
            fields: &[],
        }
    }

    pub const fn fields(self, fields: &'static [PropField]) -> Self {
        Self { fields, ..self }
    }

    pub fn field(&self, name: &str) -> Option<&'static PropField> {
        self.fields.iter().find(|field| field.name == name)
    }
}

/// One prop: what its row in the docs shows, and the control a preview arg feeding it gets.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct PropField {
    pub name: &'static str,
    /// The type as the struct declares it.
    pub ty: &'static str,
    /// `#[props(doc = "…")]`, or else the field's doc comment.
    pub doc: &'static str,
    pub default: PropDefault,
    /// `#[props(into)]`: the setter takes anything that converts into [`Self::ty`].
    pub into: bool,
    /// `#[props(some)]`: the field is an `Option` and the setter takes what it holds.
    pub some: bool,
    /// The type's control, refined by `#[props(control = …)]`. A fn rather than a value because the probe that names it is not `const`.
    pub control: fn() -> ControlKind,
}

impl PropField {
    /// A required prop, until [`Self::default`] says otherwise.
    pub const fn new(name: &'static str, ty: &'static str, control: fn() -> ControlKind) -> Self {
        Self {
            name,
            ty,
            doc: "",
            default: PropDefault::Required,
            into: false,
            some: false,
            control,
        }
    }

    pub const fn doc(self, doc: &'static str) -> Self {
        Self { doc, ..self }
    }

    pub const fn default(self, default: PropDefault) -> Self {
        Self { default, ..self }
    }

    pub const fn takes_into(self) -> Self {
        Self { into: true, ..self }
    }

    pub const fn takes_some(self) -> Self {
        Self { some: true, ..self }
    }

    pub const fn is_required(&self) -> bool {
        matches!(self.default, PropDefault::Required)
    }

    pub fn control(&self) -> ControlKind {
        (self.control)()
    }
}

/// What a prop holds when the builder is not given it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropDefault {
    /// Nothing: the builder has no `build` until it is set.
    Required,
    /// `#[props(default)]`: the type's `Default`.
    TypeDefault,
    /// `#[props(default = expr)]`, as the expression's source text.
    Expr(&'static str),
}
