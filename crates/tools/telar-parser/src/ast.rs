//! Abstract syntax tree for `.rsx` documents.

#[derive(Debug, Clone)]
pub struct RsxDocument {
    pub logic: LogicZone,
    pub style: StyleSection,
    pub view: ViewSection,
    pub previews: Vec<Preview>,
}

/// A `[preview "Name" …]` section: a named, standalone view rendered by `cargo telar preview`.
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

/// The `[style]` section: the named property bundles a view reuses.
#[derive(Debug, Clone, Default)]
pub struct StyleSection {
    pub classes: Vec<StyleClass>,
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

/// A `match <scrutinee> [as <binding>] [key <expr>]` block: the branching an `if` cannot do, because it extracts
/// a payload from the matched variant and renders a structurally different subtree per arm.
///
/// A `$`-prefixed scrutinee marks it reactive (the shown arm swaps when the value changes); a plain one chooses
/// its arm once at construction. `binding` names the matched value so `key_expr` can reach it — without a key a
/// reactive match reconciles on the variant alone, so it rebuilds when the shape changes but not when the
/// payload does.
#[derive(Debug, Clone)]
pub struct MatchBlock {
    pub scrutinee: String,
    /// The `as <name>` clause, in scope for `key_expr` only.
    pub binding: Option<String>,
    pub key_expr: Option<String>,
    pub arms: Vec<MatchArm>,
    /// 1-based `.rsx` line of the `match` header.
    pub line: usize,
    /// Byte offset in the source where the (trimmed) `scrutinee` begins.
    pub scrutinee_start: usize,
}

/// One arm: a Rust pattern and the nodes it renders. The pattern is kept raw, so a guard (`Ready(x) if x.ok()`)
/// or an or-pattern reaches rustc unchanged.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: String,
    pub body: Vec<ViewNode>,
    pub line: usize,
    /// Byte offset where `pattern` begins, so a rustc error on it maps back to this line.
    pub pattern_start: usize,
}

#[derive(Debug, Clone)]
pub enum ViewNode {
    Element(Element),
    IfBlock(IfBlock),
    ForBlock(ForBlock),
    MatchBlock(MatchBlock),
    LetStmt(LetStmt),
    /// A `//` note. Kept in the tree rather than discarded at the lexer, so the formatter puts it back where
    /// its author left it. Builds nothing.
    Comment(String),
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
    pub children: Vec<ViewNode>,
    pub line: usize,
    /// Byte offset in the source where the (de-quoted) `content` begins, or the line start when the
    /// element has no quoted content. Used to map `{…}` interpolation expressions back to source.
    pub content_start: usize,
    /// `true` when the content was written as an i18n key (`text t"nav.title"`): the transpiler emits a
    /// catalog lookup instead of a literal string.
    pub content_i18n: bool,
}

/// The form an attribute value was written in, kept rather than flattened back into a string.
///
/// This is syntax, not meaning: `12` is a [`Value::Expr`] whether it sits under `gap` or under `weight`,
/// because what a token means belongs to the key schema and the parser does not own that. What the form does
/// settle is how the text reaches the output — an expression is spliced, a quoted value expands to a
/// `format!`, a directive goes to its own parser — and every consumer used to re-derive that from the text
/// with its own `starts_with`/`contains` check.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A bare flag (`ghost`): the attribute asserts itself and carries no text.
    Flag,
    /// A Rust expression (`12`, `#3d78fa`, `$sig`, `f(x)`, `(a + b)`), read to the next space at delimiter
    /// depth 0 — so a call with arguments, or a parenthesised expression holding spaces, still reads whole.
    Expr(String),
    /// A string literal (`"Save"` or `r"…"`), de-quoted, with the escapes of the former already interpreted.
    /// Its own kind because it is *interpolating* sugar: `"Hola {name}"` expands to a `format!`, so the text
    /// is a template rather than an expression to splice.
    Quoted(String),
    /// The text between the balanced parens of the `key(…)` form, which is not Rust at all: `transition(fill
    /// 250ms ease-out)` is a clause list with its own parser and `hover_style(fill:$theme.accent)` is a
    /// nested attribute list. Reserved for the keys the view parser calls directives; every value takes the
    /// colon.
    Directive(String),
}

impl Value {
    /// The value's text, whatever form it was written in. A flag has none, so it reads as `""` — most
    /// consumers only want the text and are right not to care how it was delimited.
    pub fn text(&self) -> &str {
        match self {
            Value::Flag => "",
            Value::Expr(text) | Value::Quoted(text) | Value::Directive(text) => text,
        }
    }

    /// Whether the attribute was written as a bare flag, which is the assertion itself: `ghost`, `wrap`,
    /// `absolute`. Distinct from a value that merely happens to be empty (`label:""`).
    pub fn is_flag(&self) -> bool {
        matches!(self, Value::Flag)
    }

    /// Whether the text came from a `"…"` literal, so it is the author's data — a template the emitter
    /// expands — and never source to read identifiers out of.
    pub fn is_quoted(&self) -> bool {
        matches!(self, Value::Quoted(_))
    }

    /// Whether the author wrote a closure out in place, in either spelling Rust accepts, as opposed to
    /// forwarding a handler expression the caller was given. The transpiler wires the two to different builder
    /// methods (`.on_press` against `.maybe_on_press`), and used to ask it from two near-identical predicates
    /// of its own before the question had a value to sit on.
    pub fn is_closure(&self) -> bool {
        // The parens a closure needs to hold its spaces are the value's delimiters, not part of it, so a
        // parenthesised closure is still a closure: `on_press:(|| f())` and `on_press:|| f()` say the same.
        let text = undelimited(self.text().trim());
        !self.is_quoted()
            && (text.starts_with('|')
                || text
                    .strip_prefix("move")
                    .is_some_and(|rest| rest.trim_start().starts_with('|')))
    }
}

/// A `key:value` attribute on an element. The text is kept raw (closures included) and the [`Value`] variant
/// records which spelling delimited it.
#[derive(Debug, Clone)]
pub struct Attr {
    pub key: String,
    pub value: Value,
    /// Byte offset in the source where the value's text begins. Lets the transpiler map a closure /
    /// pass-through attribute value back to source; excluded from `PartialEq` so it stays positional metadata.
    pub value_start: usize,
}

// Equality compares semantic content only; `value_start` is positional metadata, so tests can build
// `Attr` literals with `value_start: 0` and still match a parsed attribute.
impl PartialEq for Attr {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.value == other.value
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
    /// The `row_height:<expr>` of a `virtual` loop: build only the rows the enclosing scroll viewport shows,
    /// instead of every row up front. Opt-in and explicit, because virtualising needs a fixed row height and an
    /// enclosing `scroll` — neither is something to infer from a loop that did not ask.
    pub virtual_row_height: Option<String>,
    pub body: Vec<ViewNode>,
    /// 1-based `.rsx` line of the `for` header, used to map generated code back to source.
    pub line: usize,
}

/// `expr` with one wrapping paren pair removed, or unchanged when the pair is not a wrapper.
///
/// Deliberately no tuple check: the only caller asks whether the content is a closure, and `|x, y| …` has a
/// top-level comma of its own. Stripping `(a, b)` here is harmless, since `a, b` is not a closure either.
fn undelimited(expr: &str) -> &str {
    let Some(inner) = expr.strip_prefix('(').and_then(|e| e.strip_suffix(')')) else {
        return expr;
    };
    let mut depth = 0i32;
    for c in inner.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => return expr,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    inner.trim()
}
