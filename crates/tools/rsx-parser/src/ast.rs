//! Abstract syntax tree for `.rsx` documents.

/// A fully parsed `.rsx` document with its top-level sections.
#[derive(Debug, Clone)]
pub struct RsxDocument {
    pub logic: LogicZone,
    pub props: PropsSection,
    pub style: StyleSection,
    pub view: ViewSection,
}

/// The `[props]` section: named parameters appended to the generated function signature.
#[derive(Debug, Clone, Default)]
pub struct PropsSection {
    pub params: Vec<PropParam>,
}

/// A single `name: Type` entry in `[props]`.
#[derive(Debug, Clone)]
pub struct PropParam {
    pub name: String,
    pub ty: String,
}

/// The leading Rust verbatim zone, captured untouched up to the first section header.
#[derive(Debug, Clone, Default)]
pub struct LogicZone {
    pub source: String,
}

/// The `[style]` section: a flat list of constants and style classes.
#[derive(Debug, Clone, Default)]
pub struct StyleSection {
    pub constants: Vec<StyleConst>,
    pub classes: Vec<StyleClass>,
}

/// A top-level named constant, e.g. `primary: #3d78fa` or `radius: 6`.
#[derive(Debug, Clone)]
pub struct StyleConst {
    pub name: String,
    pub value: StyleValue,
    pub line: usize,
}

/// The value attached to a style constant.
#[derive(Debug, Clone, PartialEq)]
pub enum StyleValue {
    Hex(String),
    Number(f32),
    Raw(String),
}

/// A style class, e.g. `.card` followed by indented property pairs, or an inline `.badge: ...`.
#[derive(Debug, Clone)]
pub struct StyleClass {
    pub name: String,
    pub props: Vec<StyleProp>,
    pub line: usize,
}

/// A single `key: value` property within a style class.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleProp {
    pub key: String,
    pub value: String,
}

/// The `[view]` section: the root list of nodes of the view tree.
#[derive(Debug, Clone, Default)]
pub struct ViewSection {
    pub nodes: Vec<ViewNode>,
}

/// A node within the view tree.
#[derive(Debug, Clone)]
pub enum ViewNode {
    Element(Element),
    IfBlock(IfBlock),
    ForBlock(ForBlock),
    LetStmt { source: String },
}

/// A view element: a layout container or a leaf widget.
#[derive(Debug, Clone)]
pub struct Element {
    pub tag: String,
    pub classes: Vec<String>,
    pub attrs: Vec<Attr>,
    pub content: Option<String>,
    pub canvas_params: Option<String>,
    pub children: Vec<ViewNode>,
    pub line: usize,
}

/// A `key: value` attribute on an element. The value is kept raw (closures included).
/// `is_quoted` is `true` when the value was written with double-quotes (`label:"text"`),
/// allowing callers to distinguish string literals from identifier references.
#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub key: String,
    pub value: String,
    pub is_quoted: bool,
}

/// An `if` / `else` conditional block in the view.
#[derive(Debug, Clone)]
pub struct IfBlock {
    pub condition: String,
    pub then_branch: Vec<ViewNode>,
    pub else_branch: Option<Vec<ViewNode>>,
}

/// A `for ... in ...` loop block in the view.
#[derive(Debug, Clone)]
pub struct ForBlock {
    pub pattern: String,
    pub iterable: String,
    pub body: Vec<ViewNode>,
}
