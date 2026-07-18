//! Abstract syntax tree for `.rsx` documents.

#[derive(Debug, Clone)]
pub struct RsxDocument {
    pub logic: LogicZone,
    pub style: StyleSection,
    pub view: ViewSection,
    pub previews: Vec<Preview>,
}

/// A `[preview "Name" …]` section: a named, standalone view rendered by `cargo rsx preview`.
/// Its `body` is ordinary `[view]` markup (typically a single component call with literal
/// props), so any component — including prop-taking ones — can be previewed.
#[derive(Debug, Clone)]
pub struct Preview {
    pub name: String,
    /// Header options (`width:360`, `bg:surface`, `group:"…"`, `dark`); parsed for forward
    /// compatibility but not yet consumed by the runtime. A bare flag carries an empty value.
    pub options: Vec<StyleProp>,
    pub body: Vec<ViewNode>,
    /// 1-based `.rsx` line of the `[preview …]` header.
    pub line: usize,
}

/// The leading Rust verbatim zone, captured untouched up to the first section header.
#[derive(Debug, Clone, Default)]
pub struct LogicZone {
    pub source: String,
    /// 1-based `.rsx` line of the first content line in `source` (0 when the zone is empty). Lets
    /// the transpiler map positions in the generated Rust back to the original `.rsx`.
    pub start_line: usize,
}

/// The `[style]` section: a flat list of constants and style classes.
#[derive(Debug, Clone, Default)]
pub struct StyleSection {
    pub constants: Vec<StyleConstant>,
    pub classes: Vec<StyleClass>,
}

/// A top-level named constant, e.g. `primary: #3d78fa` or `radius: 6`.
#[derive(Debug, Clone)]
pub struct StyleConstant {
    pub name: String,
    pub value: StyleValue,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StyleValue {
    Hex(String),
    Number(f32),
    Raw(String),
}

/// A style class, e.g. `@card` followed by indented property pairs, or an inline `@badge: ...`.
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

#[derive(Debug, Clone)]
pub enum ViewNode {
    Element(Element),
    IfBlock(IfBlock),
    ForBlock(ForBlock),
    LetStmt(LetStmt),
}

/// A verbatim `let` binding in the view, captured as raw Rust source.
#[derive(Debug, Clone)]
pub struct LetStmt {
    pub source: String,
    /// Byte offset in the source where `source` begins (verbatim Rust `let` statement).
    pub source_start: usize,
}

/// A view element: a layout container or a leaf widget.
#[derive(Debug, Clone)]
pub struct Element {
    pub tag: String,
    pub classes: Vec<String>,
    pub attributes: Vec<Attr>,
    pub content: Option<String>,
    /// Leading `|params|` line declared on the first deeper-indented child (before the real children).
    /// Vocabulary-neutral at the parser level; the transpiler interprets it (e.g. `canvas` drawing-area
    /// dimensions `|w, h|`).
    pub leading_params: Option<String>,
    pub children: Vec<ViewNode>,
    pub line: usize,
    /// Byte offset in the source where the (de-quoted) `content` begins, or the line start when the
    /// element has no quoted content. Used to map `{…}` interpolation expressions back to source.
    pub content_start: usize,
    /// `true` when the content was written as an i18n key (`text t"nav.title"`): the transpiler emits a
    /// catalog lookup instead of a literal string.
    pub content_i18n: bool,
}

/// A `key: value` attribute on an element. The value is kept raw (closures included).
/// `is_quoted` is `true` when the value was written with double-quotes (`label:"text"`),
/// allowing callers to distinguish string literals from identifier references.
#[derive(Debug, Clone)]
pub struct Attr {
    pub key: String,
    pub value: String,
    pub is_quoted: bool,
    /// `true` when the value was written as an i18n key (`label:t"buttons.save"`): the transpiler emits a
    /// catalog lookup instead of a literal string. Implies `is_quoted`.
    pub i18n: bool,
    /// Byte offset in the source where `value` begins. Lets the transpiler map a closure / pass-through
    /// attribute value back to source; excluded from `PartialEq` so it stays positional metadata.
    pub value_start: usize,
}

// Equality compares semantic content only; `value_start` is positional metadata, so tests can build
// `Attr` literals with `value_start: 0` and still match a parsed attribute.
impl PartialEq for Attr {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.value == other.value
            && self.is_quoted == other.is_quoted
            && self.i18n == other.i18n
    }
}

#[derive(Debug, Clone)]
pub struct IfBlock {
    pub condition: String,
    pub then_branch: Vec<ViewNode>,
    pub else_branch: Option<Vec<ViewNode>>,
    /// 1-based `.rsx` line of the `if` header, used to map generated code back to source.
    pub line: usize,
    /// Byte offset in the source where the (trimmed) `condition` begins.
    pub condition_start: usize,
}

/// A `for ... in ...` loop block in the view. A `$`-prefixed `iterable` marks a reactive list (re-run +
/// keyed reconciliation); a plain iterable is a one-time construction loop. `key_expr` is the optional
/// `key <expr>` clause giving each item a stable identity for reconciliation; without it, a reactive list
/// reconciles by position instead. `gap_expr` is the optional trailing `gap:<expr>` clause (space between
/// reconciled items), reactive-list only.
#[derive(Debug, Clone)]
pub struct ForBlock {
    pub pattern: String,
    pub iterable: String,
    pub key_expr: Option<String>,
    pub gap_expr: Option<String>,
    pub body: Vec<ViewNode>,
    /// 1-based `.rsx` line of the `for` header, used to map generated code back to source.
    pub line: usize,
}
