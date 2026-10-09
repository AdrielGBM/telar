//! The `[previews]` meta section and its `[preview]` variants: a build fn per variant and the `{STEM}_PREVIEW_ENTRIES` table that lists them, inside `::telar::__previews!` so a build without the `previews` feature compiles none of it.
//!
//! A variant reads its inputs from the canvas's `PreviewCtx`, named `__preview` in its build fn:
//!
//! - each `args(name:default)` becomes a two-way signal the body reads as `$name`, through `::telar::__preview_signal!`;
//! - each literal attribute of the root component call becomes a value its control can change, through `::telar::__preview_arg!`, unless the header says `args:none`;
//! - the root's builder logs every callback it holds in the canvas's action log.
//!
//! A `[play]` zone is part of its preview's source, and is not compiled.

use std::fmt::Write;

use telar_parser::{
    ArgDecl, Attr, Element, Preview, PreviewsMeta, RsxDocument, StyleProp, Value, ViewNode,
};
use telar_project::naming::{
    is_ident, preview_entries_const_name, preview_file_expr, preview_slug, to_pascal_case,
};
use telar_project::{
    AssetContext, MATRIX_GLOBAL_AXES, MatrixAxis, MatrixControlSize, MatrixDirection, Viewport,
    is_preview_name,
};

use crate::codegen::{Code, push_theme_binding, scheme_binding};
use crate::source_map::{ExprSpan, ShadowBinding};
use crate::split::split_top_level;
use crate::style::{format_f32, hex_to_color_expr};
use crate::theme_access::ThemeAccess;
use crate::view::{
    ViewGen, expr_marker, is_component_tag, on_source_line, props_type, resolve_source_map,
    rust_str,
};

/// What the previews of one `.rsx` are emitted against.
pub(crate) struct Previews<'a> {
    pub document: &'a RsxDocument,
    /// The snake-case name of the component the previews belong to, which names their build fns, their ids and their table.
    pub component: &'a str,
    /// The props type of that component, which describes it to the docs and refines its args' controls. `None` to read it from the root of each preview, as a previews file does.
    pub props_type: Option<&'a str>,
    /// The `.rsx` text, which each preview's source is cut from.
    pub source: &'a str,
    pub assets: Option<&'a AssetContext>,
    pub theme: ThemeAccess,
    pub theme_type: Option<&'a str>,
    /// The `.rsx` path relative to its package root, which each entry records as where it is written.
    pub rsx_path: Option<&'a str>,
}

/// The opening of the block every preview item is emitted in; [`CLOSE`] ends it.
pub(crate) const OPEN: &str = "::telar::__previews! {\n";
pub(crate) const CLOSE: &str = "}\n";

/// What the build fns copy verbatim from the `.rsx`, for the file's source map.
#[derive(Default)]
pub(crate) struct Mapped {
    pub expr_spans: Vec<ExprSpan>,
    pub shadows: Vec<ShadowBinding>,
}

/// Emits a build fn per preview, the `compile_error!` of each option or arg the previews get wrong on the line that wrote it, and the entry table, which a meta section with no preview under it has none of. Positions in what it answers are offsets in `code`.
pub(crate) fn emit_variants(code: &mut Code, previews: &Previews<'_>) -> Mapped {
    let doc = previews.document;
    let mut mapped = Mapped::default();
    let meta_options = doc
        .previews_meta
        .as_ref()
        .map_or(&[][..], |meta| &meta.options);
    let (meta, meta_errors) = Options::parse(meta_options, "[previews]");
    if let Some(meta) = &doc.previews_meta {
        push_errors(code, &meta_errors, meta.line);
    }
    if doc.previews.is_empty() {
        return mapped;
    }

    let mut entries = Vec::new();
    for (index, preview) in doc.previews.iter().enumerate() {
        let header = format!("[preview {:?}]", preview.name);
        let (own, mut errors) = Options::parse(&preview.options, &header);
        let options = own.over(&meta);
        errors.extend(options.errors(&header));

        let explicit: Vec<String> = preview.args.iter().map(|arg| arg.name.clone()).collect();
        let root = root_component(&preview.body);
        let mut view = ViewGen::new(&doc.style.classes, previews.assets)
            .with_theme_access(previews.theme)
            .with_locals(explicit.clone())
            .with_signals(explicit.clone());
        let implicit: Vec<&Attr> = match (root, options.implicit_args) {
            (Some(root), true) => view.implicit_args(root),
            _ => Vec::new(),
        };
        if let Some(root) = root {
            view = view.with_preview_root(root, options.implicit_args);
        }
        errors.extend(arg_conflicts(&header, &preview.args, &implicit));
        if let Some(Matrix::Axes(axes)) = &options.matrix {
            errors.extend(unknown_arg_axes(&header, axes, &explicit, &implicit));
        }

        let body = view.generate_root(&preview.body);
        emit_build_fn(
            code,
            previews,
            preview,
            index,
            options.fixture.as_deref(),
            &body,
            &mut mapped,
        );
        push_errors(code, &errors, preview.line);

        let entry = match id_suffix(previews.component, &doc.previews, index) {
            Ok(suffix) => Entry {
                previews,
                preview,
                index,
                suffix: &suffix,
                meta: doc.previews_meta.as_ref(),
                options: &options,
                root,
                implicit: &implicit,
            }
            .render(),
            Err(message) => format!("    compile_error!({message:?}),\n"),
        };
        entries.push((preview.line, entry));
    }

    code.push("\n", None);
    let const_name = preview_entries_const_name(previews.component);
    code.push(
        &format!("pub const {const_name}: &[::telar::preview::PreviewEntry] = &[\n"),
        None,
    );
    for (line, entry) in entries {
        let src = Some(line.saturating_sub(1) as u32);
        for text in entry.lines() {
            code.push(text, src);
            code.push("\n", src);
        }
    }
    code.push("];\n", None);
    mapped
}

/// The root of a preview's body when it is one component call: the call whose literal attributes are args and whose callbacks are logged.
fn root_component(body: &[ViewNode]) -> Option<&Element> {
    let mut nodes = body
        .iter()
        .filter(|node| !matches!(node, ViewNode::Comment(_)));
    match (nodes.next(), nodes.next()) {
        (Some(ViewNode::Element(element)), None) if is_component_tag(&element.tag) => Some(element),
        _ => None,
    }
}

fn emit_build_fn(
    code: &mut Code,
    previews: &Previews<'_>,
    preview: &Preview,
    index: usize,
    fixture: Option<&str>,
    body: &str,
    mapped: &mut Mapped,
) {
    let component = previews.component;
    code.push("\n", None);
    code.push("#[allow(dead_code, unused_variables, unused_mut)]\n", None);
    code.push(
        &format!("pub fn {component}_preview_{index}(__preview: &::telar::preview::PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {{\n"),
        None,
    );
    push_theme_binding(code, previews.theme_type);
    let binds_scheme = preview.args.iter().any(|arg| arg.name == "scheme");
    if let Some(binding) = scheme_binding(body, binds_scheme) {
        code.push(binding, None);
    }
    // A path rather than a `[logic]` name: a component's logic zone is emitted inside the component fn, which a sibling preview fn cannot see into.
    if let Some(fixture) = fixture {
        code.push(&format!("    {fixture}();\n"), None);
    }
    let signals: String = preview
        .args
        .iter()
        .map(|arg| {
            on_source_line(
                preview.line,
                &format!(
                    "    let {} = ::telar::__preview_signal!(__preview, {}, {});",
                    arg.name,
                    rust_str(&arg.name),
                    arg_default_expr(arg)
                ),
            ) + "\n"
        })
        .collect();
    let prefix = code.out.len();
    let resolved = resolve_source_map(&format!("{signals}{body}"));
    for (line, src) in &resolved.lines {
        code.push(line, *src);
        code.push("\n", *src);
    }
    for &(rel, rsx_start, len) in &resolved.expr_spans {
        mapped.expr_spans.push(ExprSpan {
            rsx_start,
            len,
            gen_start: (prefix + rel) as u32,
        });
    }
    for (rel, name) in &resolved.shadows {
        mapped.shadows.push(ShadowBinding {
            gen_decl: (prefix + rel) as u32,
            name: name.clone(),
        });
    }
    if !code.out.ends_with('\n') {
        code.push("\n", None);
    }
    code.push("}\n", None);
}

/// Each error as an item-level `compile_error!` attributed to the 1-based `.rsx` `line` of the header that wrote it.
fn push_errors(code: &mut Code, errors: &[String], line: usize) {
    let src = Some(line.saturating_sub(1) as u32);
    for error in errors {
        code.push(&format!("compile_error!({error:?});\n"), src);
    }
}

/// The `--<component>--<slug(name)>` that follows the crate name in the id of the `index`th preview, or why the preview cannot have one: an id names exactly one preview for as long as its name stays the same, so a name that slugs to nothing, or to what an earlier preview's already did, is refused rather than numbered.
fn id_suffix(component: &str, previews: &[Preview], index: usize) -> Result<String, String> {
    let name = &previews[index].name;
    let suffix = telar_project::naming::preview_id_suffix(component, name)
        .map_err(|error| error.to_string())?;
    let slug = preview_slug(name);
    if let Some(earlier) = previews[..index]
        .iter()
        .find(|earlier| preview_slug(&earlier.name) == slug)
    {
        return Err(format!(
            "[preview \"{name}\"] has the same id as [preview \"{}\"] on line {}: `{component}--{slug}`; rename one of them",
            earlier.name, earlier.line
        ));
    }
    Ok(suffix)
}

/// One element of the entry table.
struct Entry<'e> {
    previews: &'e Previews<'e>,
    preview: &'e Preview,
    index: usize,
    suffix: &'e str,
    meta: Option<&'e PreviewsMeta>,
    options: &'e Options,
    root: Option<&'e Element>,
    implicit: &'e [&'e Attr],
}

impl Entry<'_> {
    fn render(&self) -> String {
        let (component, preview) = (self.previews.component, self.preview);
        let file = match self.previews.rsx_path {
            Some(path) => preview_file_expr(path),
            None => "\"\"".to_string(),
        };
        let mut entry = format!(
            "    ::telar::preview::PreviewEntry::new(concat!(env!(\"CARGO_CRATE_NAME\"), {:?}), {component:?}, {:?}, {component}_preview_{})\n        .location({file}, {})",
            self.suffix, preview.name, self.index, preview.line
        );
        let mut push = |setter: String| {
            let _ = write!(entry, "\n        .{setter}");
        };
        if let Some(title) = self.meta.and_then(|meta| meta.title.as_deref()) {
            push(format!("title({title:?})"));
        }
        if let Some(docs) = self
            .meta
            .map(|meta| meta.body.as_str())
            .filter(|body| !body.is_empty())
        {
            push(format!("docs({docs:?})"));
        }
        let source = excerpt(
            self.previews.source,
            &self.previews.document.previews,
            self.index,
        );
        push(format!(
            "source({source:?}, &[::telar::preview::SourceSpan::new(0, {}, {})])",
            source.len(),
            preview.line
        ));
        push(format!(
            "props(<{} as ::telar::preview::HasPropsSchema>::schema)",
            self.props_type()
        ));
        let specs = self.arg_specs();
        if !specs.is_empty() {
            push(format!("args(&[{}])", specs.join(", ")));
        }
        self.options.push_setters(&mut push);
        entry.push_str(",\n");
        entry
    }

    /// The props type of the component under preview: the file's own, or the one the root names when the file previews a component written elsewhere.
    fn props_type(&self) -> String {
        if let Some(props) = self.previews.props_type {
            return props.to_string();
        }
        let component = self.previews.component;
        match self.root {
            Some(root) if root.tag.rsplit("::").next() == Some(component) => props_type(&root.tag),
            _ => to_pascal_case(component) + "Props",
        }
    }

    /// The args the preview declares ahead of being built, so each has its control before the first build: the explicit ones live, the implicit ones read at build. A default is declared only where its text form is certain from the literal.
    fn arg_specs(&self) -> Vec<String> {
        let explicit = self.preview.args.iter().map(|arg| {
            let spec = format!(
                "::telar::preview::ArgSpec::new({}).binding(::telar::preview::ArgBinding::Live)",
                rust_str(&arg.name)
            );
            with_default(spec, arg_value_text(&arg.default, Numbers::AsWritten))
        });
        let implicit = self.implicit.iter().map(|attr| {
            let spec = format!("::telar::preview::ArgSpec::new({})", rust_str(&attr.key));
            with_default(spec, arg_value_text(&attr.value, Numbers::AsFloat))
        });
        explicit.chain(implicit).collect()
    }
}

fn with_default(spec: String, default: Option<String>) -> String {
    match default {
        Some(text) => format!("{spec}.default({})", rust_str(&text)),
        None => spec,
    }
}

/// The text of the `index`th preview as written, from its header to the next one or the end of the file, without trailing blank lines.
fn excerpt<'s>(source: &'s str, previews: &[Preview], index: usize) -> &'s str {
    let start = line_offset(source, previews[index].line);
    let end = previews
        .get(index + 1)
        .map_or(source.len(), |next| line_offset(source, next.line));
    source.get(start..end).unwrap_or("").trim_end()
}

/// The byte offset where the 1-based `line` of `source` starts, or the end of `source` past its last line.
fn line_offset(source: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    source
        .match_indices('\n')
        .nth(line - 2)
        .map_or(source.len(), |(at, _)| at + 1)
}

/// The Rust a declared arg starts from: a quoted default is a string, a hex colour a `Color`, and anything else the expression as written, mapped back to where it was written.
fn arg_default_expr(arg: &ArgDecl) -> String {
    match &arg.default {
        Value::Quoted(text) => rust_str(text),
        Value::Flag => "true".to_string(),
        Value::Expr(text) | Value::Directive(text) => {
            let text = text.trim();
            if text.starts_with('#') && telar_parser::parse_hex(text).is_some() {
                return hex_to_color_expr(text);
            }
            format!("{}{text}", expr_marker(arg.default_start, text.len()))
        }
    }
}

/// Whether a root attribute's value is a literal, which a preview reads as an arg: a string, a flag, a bool, a number, a hex colour or a path to a constant such as `Variant::Primary`.
pub(crate) fn is_literal(value: &Value) -> bool {
    match value {
        Value::Flag | Value::Quoted(_) => true,
        Value::Expr(text) => literal_expr(text.trim()).is_some(),
        Value::Directive(_) => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Literal {
    Bool,
    Number,
    Color,
    Path,
}

fn literal_expr(text: &str) -> Option<Literal> {
    if text == "true" || text == "false" {
        return Some(Literal::Bool);
    }
    if text.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
        && (text.parse::<f32>().is_ok_and(f32::is_finite) || arg_number(text).is_some())
    {
        return Some(Literal::Number);
    }
    if text.starts_with('#') {
        return telar_parser::parse_hex(text).map(|_| Literal::Color);
    }
    let mut segments = text.split("::");
    let is_path = segments.clone().count() > 1 && segments.all(is_ident);
    is_path.then_some(Literal::Path)
}

/// How a number reaches the arg it is the default of: as written, for an `args(…)` default, which is a Rust expression; or as an `f32` literal, the way a component attribute's number is emitted.
#[derive(Clone, Copy)]
enum Numbers {
    AsWritten,
    AsFloat,
}

/// The arg value a literal starts as, in the text form a control, a link and a hot reload carry, or `None` where only the arg's type can say: a path to a constant, or an expression.
fn arg_value_text(value: &Value, numbers: Numbers) -> Option<String> {
    let text = match value {
        Value::Flag => return Some("true".to_string()),
        Value::Quoted(text) => return Some(quoted_arg_text(text)),
        Value::Expr(text) => text.trim(),
        Value::Directive(_) => return None,
    };
    match literal_expr(text)? {
        Literal::Bool => Some(text.to_string()),
        Literal::Color => {
            let [r, g, b, a] = telar_parser::parse_hex(text)?;
            Some(match a {
                255 => format!("#{r:02x}{g:02x}{b:02x}"),
                _ => format!("#{r:02x}{g:02x}{b:02x}{a:02x}"),
            })
        }
        Literal::Number => match (numbers, text.parse::<f32>()) {
            (Numbers::AsFloat, Ok(number)) => Some(format_f32(number)),
            _ => arg_number(text),
        },
        Literal::Path => None,
    }
}

/// `text` quoted the way the arg value text form writes a string.
fn quoted_arg_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

const NUMBER_SUFFIXES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64",
];

/// The Rust number literal `text` in the arg value text form, its type suffix dropped: `3u32` is `3`.
fn arg_number(text: &str) -> Option<String> {
    let number = NUMBER_SUFFIXES
        .iter()
        .find_map(|suffix| text.strip_suffix(suffix))
        .unwrap_or(text);
    is_arg_number(number).then(|| number.to_string())
}

/// Whether `text` is a number as the arg value text form writes one: an integer, or a float carrying a `.` or an exponent, with neither a suffix nor a `_`.
fn is_arg_number(text: &str) -> bool {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    if digits.is_empty() {
        return false;
    }
    if digits.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    digits.starts_with(|c: char| c.is_ascii_digit())
        && digits.contains(['.', 'e', 'E'])
        && text.parse::<f64>().is_ok()
}

/// An explicit arg that a literal attribute of the root also sets, which would give one control two values.
fn arg_conflicts(header: &str, args: &[ArgDecl], implicit: &[&Attr]) -> Vec<String> {
    args.iter()
        .filter(|arg| implicit.iter().any(|attr| attr.key == arg.name))
        .map(|arg| {
            format!(
                "{header} declares `{name}` in `args(…)` and also sets it as a literal on its root: bind the root to it with `{name}:${name}`, or leave it out of `args(…)`",
                name = arg.name
            )
        })
        .collect()
}

/// A matrix's arg axis that names no arg of the preview, so its cells would differ in nothing.
fn unknown_arg_axes(
    header: &str,
    axes: &[MatrixAxis],
    explicit: &[String],
    implicit: &[&Attr],
) -> Vec<String> {
    axes.iter()
        .filter_map(|axis| match axis {
            MatrixAxis::Arg { name, .. } => Some(name),
            _ => None,
        })
        .filter(|name| {
            !explicit.contains(name) && !implicit.iter().any(|attr| &attr.key == *name)
        })
        .map(|name| {
            format!(
                "{header} `matrix:` varies `{name}`, which is neither one of {} nor an arg of this preview: declare it in `args(…)`, or write it as a literal attribute of the root component",
                MATRIX_GLOBAL_AXES.join(", ")
            )
        })
        .collect()
}

/// The header options of a `[previews]` meta section or a `[preview]` variant, read and checked. A variant's own options stand over the meta section's, which every variant shares.
#[derive(Clone, Default)]
struct Options {
    layout: Option<&'static str>,
    viewport: Option<(f32, f32)>,
    bg: Option<String>,
    mode: Option<String>,
    locale: Option<String>,
    dir: Option<&'static str>,
    tags: Vec<String>,
    matrix: Option<Matrix>,
    decorator: Option<String>,
    fixture: Option<String>,
    surface: Option<(f32, f32)>,
    animate: bool,
    /// `false` when the header says `args:none`.
    implicit_args: bool,
    /// Whether `args:` was written, so a variant's says more than the meta section's.
    args_written: bool,
}

/// A matrix a header names or writes out.
#[derive(Clone)]
enum Matrix {
    Named(String),
    Axes(Vec<MatrixAxis>),
}

/// Every option a preview header takes, in the order a message lists them. `args(name:default …)` is the one form that is not `key:value`.
pub const PREVIEW_OPTION_KEYS: &[&str] = &[
    "layout",
    "viewport",
    "bg",
    "mode",
    "locale",
    "dir",
    "tags",
    "matrix",
    "decorator",
    "fixture",
    "surface",
    "animate",
    "args",
];

impl Options {
    fn parse(options: &[StyleProp], header: &str) -> (Self, Vec<String>) {
        let mut parsed = Options {
            implicit_args: true,
            ..Options::default()
        };
        let mut errors = Vec::new();
        for (index, option) in options.iter().enumerate() {
            let key = option.key.as_str();
            if options[..index].iter().any(|earlier| earlier.key == key) {
                errors.push(format!("{header} sets `{key}` twice"));
                continue;
            }
            if let Err(message) = parsed.read(key, option.value.trim()) {
                errors.push(format!("{header} {message}"));
            }
        }
        (parsed, errors)
    }

    /// Reads one option into `self`, or says why it cannot be read.
    fn read(&mut self, key: &str, value: &str) -> Result<(), String> {
        if key == "animate" {
            return match value.is_empty() {
                true => {
                    self.animate = true;
                    Ok(())
                }
                false => {
                    Err("`animate` is a flag: write it bare, beside `surface:WIDTHxHEIGHT`".into())
                }
            };
        }
        if !PREVIEW_OPTION_KEYS.contains(&key) {
            return Err(format!(
                "has no option `{key}`: a preview header takes {}",
                PREVIEW_OPTION_KEYS.join(", ")
            ));
        }
        if value.is_empty() {
            return Err(format!("`{key}` needs a value, like `{}`", example(key)));
        }
        let text = unquoted(value);
        match key {
            "layout" => {
                self.layout = Some(match text {
                    "padded" => "Padded",
                    "centered" => "Centered",
                    "fullscreen" => "Fullscreen",
                    _ => {
                        return Err(format!(
                            "`layout:{text}` is not a layout: write padded, centered or fullscreen"
                        ));
                    }
                })
            }
            "viewport" => {
                let size = Viewport::parse(text).ok_or_else(|| {
                    format!("`viewport:{text}` is not a size: write it WIDTHxHEIGHT, like viewport:390x844")
                })?;
                self.viewport = Some((size.width as f32, size.height as f32));
            }
            "bg" => {
                if telar_parser::parse_hex(text).is_none() || !text.starts_with('#') {
                    return Err(format!(
                        "`bg:{text}` is not a colour: write it in hex, like bg:#202020"
                    ));
                }
                self.bg = Some(hex_to_color_expr(text));
            }
            "mode" => self.mode = Some(text.to_string()),
            "locale" => self.locale = Some(text.to_string()),
            "dir" => {
                self.dir = Some(match MatrixDirection::parse(text) {
                    Some(MatrixDirection::Ltr) => "Ltr",
                    Some(MatrixDirection::Rtl) => "Rtl",
                    None => {
                        return Err(format!("`dir:{text}` is not a direction: write ltr or rtl"));
                    }
                })
            }
            "tags" => {
                let tags = list(value);
                if tags.iter().any(String::is_empty) || tags.is_empty() {
                    return Err("`tags:` lists names, like tags:[stateful slow]".into());
                }
                self.tags = tags;
            }
            "matrix" => self.matrix = Some(Matrix::parse(value)?),
            "decorator" => self.decorator = Some(text.to_string()),
            "fixture" => self.fixture = Some(text.to_string()),
            "surface" => {
                let size = text.split_once(['x', 'X']).and_then(|(w, h)| {
                    Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?))
                });
                self.surface = Some(size.ok_or_else(|| {
                    format!("surface: expects WIDTHxHEIGHT, e.g. surface:360x240 (got {text})")
                })?);
            }
            "args" => {
                if text != "none" {
                    return Err(format!(
                        "`args:{text}` is not an option: `args:none` keeps the root's literals fixed, and `args(name:default …)` declares args"
                    ));
                }
                self.implicit_args = false;
                self.args_written = true;
            }
            _ => unreachable!("every key in PREVIEW_OPTION_KEYS is read above"),
        }
        Ok(())
    }

    /// These options over `base`'s, each one set here winning; tags add up, and a surface brings its own `animate`.
    fn over(&self, base: &Options) -> Options {
        let (surface, animate) = match self.surface {
            Some(surface) => (Some(surface), self.animate),
            None => (base.surface, base.animate || self.animate),
        };
        let mut tags = base.tags.clone();
        tags.extend(
            self.tags
                .iter()
                .filter(|tag| !base.tags.contains(tag))
                .cloned(),
        );
        Options {
            layout: self.layout.or(base.layout),
            viewport: self.viewport.or(base.viewport),
            bg: self.bg.clone().or_else(|| base.bg.clone()),
            mode: self.mode.clone().or_else(|| base.mode.clone()),
            locale: self.locale.clone().or_else(|| base.locale.clone()),
            dir: self.dir.or(base.dir),
            tags,
            matrix: self.matrix.clone().or_else(|| base.matrix.clone()),
            decorator: self.decorator.clone().or_else(|| base.decorator.clone()),
            fixture: self.fixture.clone().or_else(|| base.fixture.clone()),
            surface,
            animate,
            implicit_args: match self.args_written {
                true => self.implicit_args,
                false => base.implicit_args,
            },
            args_written: self.args_written || base.args_written,
        }
    }

    /// What the options get wrong together.
    fn errors(&self, header: &str) -> Vec<String> {
        match self.animate && self.surface.is_none() {
            true => vec![format!(
                "{header} `animate` plays a surface's enter transition, so it goes with `surface:WIDTHxHEIGHT`"
            )],
            false => Vec::new(),
        }
    }

    /// Hands `push` each entry setter the options ask for.
    fn push_setters(&self, push: &mut impl FnMut(String)) {
        if let Some(layout) = self.layout {
            push(format!("layout(::telar::preview::Layout::{layout})"));
        }
        if let Some((width, height)) = self.viewport {
            push(format!("viewport({width:?}, {height:?})"));
        }
        if let Some(color) = &self.bg {
            push(format!("bg({color})"));
        }
        if let Some(mode) = &self.mode {
            push(format!("mode({mode:?})"));
        }
        if let Some(locale) = &self.locale {
            push(format!("locale({locale:?})"));
        }
        if let Some(dir) = self.dir {
            push(format!("dir(::telar::Direction::{dir})"));
        }
        if !self.tags.is_empty() {
            push(format!("tags(&{:?})", self.tags));
        }
        if let Some((width, height)) = self.surface {
            let surface = format!("::telar::preview::PreviewSurface::new({width:?}, {height:?})");
            push(match self.animate {
                true => format!("surface({surface}.animated())"),
                false => format!("surface({surface})"),
            });
        }
        if let Some(decorator) = &self.decorator {
            push(format!("decorate({decorator})"));
        }
        if let Some(matrix) = &self.matrix {
            push(format!("matrix({})", matrix.expr()));
        }
    }
}

fn example(key: &str) -> &'static str {
    match key {
        "layout" => "layout:centered",
        "viewport" => "viewport:390x844",
        "bg" => "bg:#202020",
        "mode" => "mode:dark",
        "locale" => "locale:ar",
        "dir" => "dir:rtl",
        "tags" => "tags:[stateful]",
        "matrix" => "matrix:themes",
        "decorator" => "decorator:frames::card",
        "fixture" => "fixture:seed_state",
        "surface" => "surface:360x240",
        _ => "args:none",
    }
}

/// `value` without the quotes around it, if it is quoted.
fn unquoted(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
}

/// The items of `[a b "c d"]` as written, or the one item of a value written bare.
fn items(value: &str) -> Vec<&str> {
    let value = value.trim();
    match value
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
    {
        Some(inner) => split_top_level(inner, char::is_whitespace, false),
        None => vec![value],
    }
}

/// [`items`], each unquoted.
fn list(value: &str) -> Vec<String> {
    items(value)
        .into_iter()
        .map(|item| unquoted(item).to_string())
        .collect()
}

impl Matrix {
    /// `themes`, a matrix named in `[telar.previews.matrices]` or built in, or `(mode:[light dark] size:[12 16])`, its axes written out, the first outermost.
    fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        let Some(inner) = value
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix(')'))
        else {
            let name = unquoted(value);
            return match is_preview_name(name) {
                true => Ok(Matrix::Named(name.to_string())),
                false => Err(format!(
                    "`matrix:{value}` is neither a matrix's name, like matrix:themes, nor its axes, like matrix:(mode:[light dark] size:[12 16])"
                )),
            };
        };
        let mut axes: Vec<MatrixAxis> = Vec::new();
        for token in split_top_level(inner, char::is_whitespace, false) {
            let Some((name, values)) = token.split_once(':') else {
                return Err(format!(
                    "`matrix:` axis `{token}` needs its values, like `{token}:[a b]`"
                ));
            };
            if axes.iter().any(|axis| axis.name() == name) {
                return Err(format!("`matrix:` varies `{name}` twice"));
            }
            axes.push(axis(name, &items(values))?);
        }
        match axes.is_empty() {
            true => Err(
                "`matrix:()` varies nothing: give it an axis, like matrix:(mode:[light dark])"
                    .into(),
            ),
            false => Ok(Matrix::Axes(axes)),
        }
    }

    fn expr(&self) -> String {
        match self {
            Matrix::Named(name) => format!("::telar::preview::Matrix::Named({name:?})"),
            Matrix::Axes(axes) => {
                let axes: Vec<String> = axes.iter().map(axis_expr).collect();
                format!("::telar::preview::Matrix::Axes(&[{}])", axes.join(", "))
            }
        }
    }
}

/// One axis of an inline matrix, read and checked. A viewport name is left for the package's `[telar.previews.viewports]` to resolve as the matrix expands, since a file transpiles knowing nothing of its package's manifest.
fn axis(name: &str, written: &[&str]) -> Result<MatrixAxis, String> {
    let at = format!("`matrix:` axis `{name}`");
    let is_arg = !MATRIX_GLOBAL_AXES.contains(&name);
    // An arg's values are arg value text, where the quotes are what make `"Save"` text rather than the variant `Save`.
    let values: Vec<String> = written
        .iter()
        .map(|value| match is_arg {
            true => value.to_string(),
            false => unquoted(value).to_string(),
        })
        .collect();
    if values.is_empty() || values.iter().any(String::is_empty) {
        return Err(format!("{at} lists no values"));
    }
    if let Some((_, repeated)) = values
        .iter()
        .enumerate()
        .find(|(index, value)| values[..*index].contains(value))
    {
        return Err(format!("{at} lists `{repeated}` more than once"));
    }
    let each = |read: fn(&str) -> bool, expected: &str| {
        values
            .iter()
            .map(|value| match read(value.as_str()) {
                true => Ok(value.clone()),
                false => Err(format!("{at}: `{value}` is not {expected}")),
            })
            .collect::<Result<Vec<String>, String>>()
    };
    Ok(match name {
        "mode" => MatrixAxis::Mode(values),
        "locale" => MatrixAxis::Locale(values),
        "dir" => MatrixAxis::Dir(
            values
                .iter()
                .map(|value| {
                    MatrixDirection::parse(value)
                        .ok_or_else(|| format!("{at}: `{value}` is not ltr or rtl"))
                })
                .collect::<Result<_, _>>()?,
        ),
        "viewport" => MatrixAxis::Viewport(each(
            |value| Viewport::parse(value).is_some() || is_preview_name(value),
            "a size like 390x844 nor a name in `[telar.previews.viewports]`",
        )?),
        "control_size" => MatrixAxis::ControlSize(
            values
                .iter()
                .map(|value| {
                    MatrixControlSize::parse(value).ok_or_else(|| {
                        format!("{at}: `{value}` is not one of mini, small, regular, large")
                    })
                })
                .collect::<Result<_, _>>()?,
        ),
        _ if is_ident(name) => MatrixAxis::Arg {
            name: name.to_string(),
            values: each(
                is_arg_value_text,
                "an arg value: a bool, a number, a hex colour, a variant name or text in quotes",
            )?,
        },
        _ => {
            return Err(format!(
                "{at} is neither one of {} nor an arg name",
                MATRIX_GLOBAL_AXES.join(", ")
            ));
        }
    })
}

/// Whether `text` is an arg value as a matrix writes one: `true`, `none`, `12`, `1.5`, `#ff8800`, `Primary` or `"Save"`.
fn is_arg_value_text(text: &str) -> bool {
    match text {
        "true" | "false" | "none" => true,
        _ if text.starts_with('"') => text.len() > 1 && text.ends_with('"'),
        _ if text.starts_with('#') => telar_parser::parse_hex(text).is_some(),
        _ if text.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+') => {
            is_arg_number(text)
        }
        _ => is_ident(text),
    }
}

fn axis_expr(axis: &MatrixAxis) -> String {
    let path = "::telar::preview::Axis";
    match axis {
        MatrixAxis::Mode(modes) => format!("{path}::Mode(&{modes:?})"),
        MatrixAxis::Locale(locales) => format!("{path}::Locale(&{locales:?})"),
        MatrixAxis::Dir(directions) => {
            let directions: Vec<&str> = directions
                .iter()
                .map(|direction| match direction {
                    MatrixDirection::Ltr => "::telar::Direction::Ltr",
                    MatrixDirection::Rtl => "::telar::Direction::Rtl",
                })
                .collect();
            format!("{path}::Dir(&[{}])", directions.join(", "))
        }
        MatrixAxis::Viewport(viewports) => format!("{path}::Viewport(&{viewports:?})"),
        MatrixAxis::ControlSize(sizes) => {
            let sizes: Vec<&str> = sizes
                .iter()
                .map(|size| match size {
                    MatrixControlSize::Mini => "::telar::ControlSize::Mini",
                    MatrixControlSize::Small => "::telar::ControlSize::Small",
                    MatrixControlSize::Regular => "::telar::ControlSize::Regular",
                    MatrixControlSize::Large => "::telar::ControlSize::Large",
                })
                .collect();
            format!("{path}::ControlSize(&[{}])", sizes.join(", "))
        }
        MatrixAxis::Arg { name, values } => format!("{path}::Arg({name:?}, &{values:?})"),
    }
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod tests;
