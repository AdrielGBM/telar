//! `cargo telar migrate` — rewrites a project's `.rsx` files into the one value grammar.
//!
//! Every rewrite here is mechanical: a spelling that used to mean something the language no longer has a second way to say. What is *not* mechanical is reported instead of guessed — a `build "…"` or `widget "…"` needs names for positional arguments, and only a person knows them.
//!
//! Run it once per project, then `cargo telar fmt` and the usual build. It is idempotent: a file already in the new grammar comes out byte-identical, which is what makes `--check` a CI answer.
//!
//! Quoted text is left alone throughout. A `"…"` is the author's data, and a documentation string showing the old spelling is prose about the language rather than a use of it — rewriting one would change what a sentence says.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::cli::MigrateArgs;

pub(crate) fn run_migrate_cmd(args: MigrateArgs) {
    let roots = match args.paths.is_empty() {
        true => vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))],
        false => args.paths.clone(),
    };

    let mut sources = Vec::new();
    for root in &roots {
        collect_rsx(root, &mut sources);
    }
    sources.sort();
    sources.dedup();

    let modules = component_modules(&sources);
    let reactive = reactive_props(&sources);
    let (mut changed, mut failed, mut manual) = (Vec::new(), Vec::new(), Vec::new());
    for path in &sources {
        let Ok(source) = std::fs::read_to_string(path) else {
            failed.push(path.clone());
            continue;
        };
        manual.extend(escapes_needing_a_person(path, &source));
        let migrated = migrate(&source, &modules, own_stem(path), &reactive);
        if migrated == source {
            continue;
        }
        changed.push(path.clone());
        if !args.check && std::fs::write(path, &migrated).is_err() {
            failed.push(path.clone());
        }
    }

    for (path, line, text) in &manual {
        println!("{}:{line}: {text}", display(path));
    }
    if !manual.is_empty() {
        println!(
            "[cargo-telar] {} escape(s) need a component with named props — converting them is hand work",
            manual.len()
        );
    }
    for path in &changed {
        println!("{}", display(path));
    }
    for path in &failed {
        eprintln!(
            "[cargo-telar] {}: could not be read or written",
            display(path)
        );
    }
    let verb = match args.check {
        true => "would be rewritten",
        false => "rewritten",
    };
    println!(
        "[cargo-telar] {} of {} file(s) {verb}",
        changed.len(),
        sources.len()
    );
    if !failed.is_empty() || (args.check && !changed.is_empty()) {
        std::process::exit(1);
    }
}

/// Every rewrite, in the order the later ones depend on: the colon form first, so what follows reads one grammar rather than two.
fn migrate(
    source: &str,
    modules: &BTreeMap<String, String>,
    own: &str,
    reactive: &BTreeMap<String, Vec<String>>,
) -> String {
    // A file that binds `theme` itself is not talking about the view's handle: its own binding shadows it, so leaving it alone keeps exactly the behaviour the file had.
    let binds_own_theme = zones(source).iter().any(|z| {
        z.section == Section::Logic
            && z.body
                .lines()
                .any(|line| line.starts_with("let theme =") || line.starts_with("let theme:"))
    });
    let read_theme = |body: &str| match binds_own_theme {
        true => body.to_string(),
        false => theme_reads(body),
    };

    let mut out = String::with_capacity(source.len());
    for zone in zones(source) {
        let body = match zone.section {
            Section::View | Section::Preview => {
                let body = colonise(zone.body);
                let body = i18n_macro(&body);
                let body = read_theme(&body);
                reactive_closures(&clip_shapes(&body), reactive)
            }
            Section::Style => read_theme(zone.body),
            Section::Logic => match binds_own_theme {
                true => shared_handlers(zone.body),
                false => shared_handlers(&theme_calls(zone.body)),
            },
            Section::None => zone.body.to_string(),
        };
        out.push_str(zone.header);
        out.push_str(&body);
    }
    style_constants_to_logic(&imports_for_tags(&out, modules, own))
}

/// `tag key:(|| …)` → `tag key:(Reactive::of(|| …))` for the props this sweep turned into a `Reactive`.
///
/// A closure was how a call site said "a value that changes", and it fitted the `Box<dyn Fn() -> T>` the prop used to be. `Reactive<T>` cannot take one through `Into` — the blanket `From<T>` already claims every type — so the wrapper is spelled out. Only for props this pass rewrote, by tag and by name: any other closure is a handler and stays one.
fn reactive_closures(body: &str, reactive: &BTreeMap<String, Vec<String>>) -> String {
    let mut out = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        let Some(props) = leading_tag(line).and_then(|tag| reactive.get(tag)) else {
            out.push_str(line);
            continue;
        };
        let mut rewritten = line.to_string();
        for prop in props {
            let needle = format!("{prop}:(");
            let Some(at) = rewritten.find(&needle) else {
                continue;
            };
            let open = at + needle.len() - 1;
            let Some(close) = closing_paren(rewritten.as_bytes(), open) else {
                continue;
            };
            let inner = &rewritten[open + 1..close];
            if !inner.trim_start().starts_with('|') && !inner.trim_start().starts_with("move |") {
                continue;
            }
            rewritten = format!(
                "{}{prop}:(Reactive::of({inner})){}",
                &rewritten[..at],
                &rewritten[close + 1..]
            );
        }
        out.push_str(&rewritten);
    }
    out
}

/// The props each component turned into a `Reactive`, by file stem — read before anything is rewritten, because a call site is in a different file from the declaration it has to agree with.
fn reactive_props(sources: &[PathBuf]) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for path in sources {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let Some(at) = source.find("pub struct Props {") else {
            continue;
        };
        let Some(end) = source[at..].find("\n}").map(|i| at + i) else {
            continue;
        };
        let (_, names) = rewrite_boxed_props(&source[at..end]);
        if !names.is_empty() {
            out.insert(own_stem(path).to_string(), names);
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    None,
    Logic,
    Style,
    View,
    Preview,
}

struct Zone<'a> {
    section: Section,
    /// The `[section]` line itself, kept out of the body so a rewrite can never touch it.
    header: &'a str,
    body: &'a str,
}

fn zones(source: &str) -> Vec<Zone<'_>> {
    let mut out = Vec::new();
    let (mut section, mut header_at, mut body_at) = (Section::None, 0usize, 0usize);
    let mut at = 0usize;
    for line in source.split_inclusive('\n') {
        if let Some(next) = section_of(line) {
            out.push(Zone {
                section,
                header: &source[header_at..body_at],
                body: &source[body_at..at],
            });
            (section, header_at, body_at) = (next, at, at + line.len());
        }
        at += line.len();
    }
    out.push(Zone {
        section,
        header: &source[header_at..body_at],
        body: &source[body_at..],
    });
    out
}

fn section_of(line: &str) -> Option<Section> {
    let t = line.trim();
    match t {
        "[logic]" => Some(Section::Logic),
        "[style]" => Some(Section::Style),
        "[view]" => Some(Section::View),
        _ if t.starts_with("[preview") && t.ends_with(']') => Some(Section::Preview),
        _ => None,
    }
}

/// The keys whose `key(…)` is a grammar of its own. Everything else is a value and takes the colon.
const DIRECTIVES: &[&str] = &[
    "transition",
    "hover_style",
    "active_style",
    "disabled_style",
    "focus_style",
    "cols",
    "stroke_width",
    "drag_button",
];

/// `key(expr)` → `key:expr`, parenthesised only where the expression holds a top-level space — which is what the parens are for now, and they are ordinary Rust rather than punctuation the DSL invented.
fn colonise(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        out.push_str(&colonise_line(line));
    }
    out
}

fn colonise_line(line: &str) -> String {
    // A control-flow line is Rust, not an attribute list: `if shown($seen)` and a `[view]`-level `let` both hold calls, and a call is not a key however much the shape rhymes.
    if leading_token(line).is_some_and(is_control_flow) {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        if let Some(end) = string_end(bytes, i) {
            out.push_str(&line[i..end]);
            i = end;
            continue;
        }
        // A value already in colon form is one token: `on_press:(|| f())` holds a call, and a call is not an attribute however much it looks like one.
        if bytes[i] == b'(' && i > 0 && bytes[i - 1] == b':' {
            let end = closing_paren(bytes, i).map(|c| c + 1).unwrap_or(line.len());
            out.push_str(&line[i..end]);
            i = end;
            continue;
        }
        // One char, not one byte: a `.rsx` line holds prose, and an em dash outside a string literal is three of them.
        let step = line[i..].chars().next().unwrap().len_utf8();
        let Some((key, open)) = key_before_paren(line, i) else {
            out.push_str(&line[i..i + step]);
            i += step;
            continue;
        };
        let Some(close) = closing_paren(bytes, open) else {
            out.push_str(&line[i..i + step]);
            i += step;
            continue;
        };
        if DIRECTIVES.contains(&key) {
            out.push_str(&line[i..=close]);
            i = close + 1;
            continue;
        }
        let inner = &line[open + 1..close];
        // The key is already in `out` — the walk reaches its `(` one byte at a time.
        out.truncate(out.len() - key.len());
        out.push_str(key);
        out.push(':');
        // Parenthesised when the expression holds a top-level space, and when it opens with `::`: a leading path separator against the colon reads as `key::…`, which is a key nobody wrote.
        match top_level_space(inner) || inner.trim_start().starts_with("::") {
            true => out.push_str(&format!("({inner})")),
            false => out.push_str(inner),
        }
        i = close + 1;
    }
    out
}

/// The keywords that open a Rust line rather than an element. `match` and `else` carry no parenthesised value of their own, but a scrutinee or a guard on the same line does.
fn is_control_flow(word: &str) -> bool {
    matches!(word, "if" | "else" | "for" | "let" | "match" | "while")
}

/// The key immediately before the `(` at or after `i`, when that `(` opens an attribute's value.
fn key_before_paren(line: &str, i: usize) -> Option<(&str, usize)> {
    let bytes = line.as_bytes();
    if bytes[i] != b'(' {
        return None;
    }
    let start = line[..i]
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|at| at + 1)
        .unwrap_or(0);
    let key = &line[start..i];
    let leads_ok = key
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
    // A key is preceded by whitespace: `f(x)` inside a value is a call, not an attribute.
    let preceded_by_space = start == 0 || bytes[start - 1].is_ascii_whitespace();
    (!key.is_empty() && leads_ok && preceded_by_space).then_some((key, i))
}

fn closing_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        if let Some(end) = string_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The index just past the string literal starting at `i`, or `None` when nothing starts there.
fn string_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] != b'"' {
        return None;
    }
    let mut j = i + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 2,
            b'"' => return Some(j + 1),
            _ => j += 1,
        }
    }
    Some(bytes.len())
}

fn top_level_space(s: &str) -> bool {
    let bytes = s.as_bytes();
    let (mut depth, mut i) = (0i32, 0usize);
    while i < s.len() {
        if let Some(end) = string_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            c if c.is_ascii_whitespace() && depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// `key:t"nav.title"` → `key:t!("nav.title")`. The *content* position keeps `t"…"`, because there the literal is the syntax rather than a value.
fn i18n_macro(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find(":t\"") {
        let Some(end) = string_end(rest.as_bytes(), at + 2) else {
            break;
        };
        out.push_str(&rest[..at + 1]);
        out.push_str("t!(");
        out.push_str(&rest[at + 2..end]);
        out.push(')');
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// `theme.primary` → `$theme.primary`. The view binds `theme` as a handle, so a theme read is the same `$` that reads a signal — and it re-reads where it is written instead of freezing at construction.
fn theme_reads(body: &str) -> String {
    replace_outside_strings(body, |chunk| replace_theme_name(chunk, "$theme"))
}

/// A bare `theme()` → `theme.get()` in `[logic]`, which sits below the binding and so no longer sees the crate's own accessor function. A qualified `crate::core::theme::theme()` still names that function and is left alone — which is also what a nested `fn` inside `[logic]` needs, since it cannot see the binding.
fn theme_calls(body: &str) -> String {
    replace_outside_strings(body, |chunk| {
        let bytes = chunk.as_bytes();
        let (mut out, mut i) = (String::with_capacity(chunk.len()), 0usize);
        while i < chunk.len() {
            if chunk[i..].starts_with("theme()") && starts_a_name(bytes, i) {
                out.push_str("theme.get()");
                i += "theme()".len();
                continue;
            }
            let ch = chunk[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    })
}

fn replace_theme_name(chunk: &str, to: &str) -> String {
    let bytes = chunk.as_bytes();
    let (mut out, mut i) = (String::with_capacity(chunk.len()), 0usize);
    while i < chunk.len() {
        if chunk[i..].starts_with("theme")
            && starts_a_name(bytes, i)
            && bytes.get(5 + i) == Some(&b'.')
        {
            out.push_str(to);
            i += "theme".len();
            continue;
        }
        let ch = chunk[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Whether a name may begin at `i`: not the tail of a longer one, not a field of something (`row.theme`), not already sigiled, and not a segment of a path (`crate::theme::…`).
fn starts_a_name(bytes: &[u8], i: usize) -> bool {
    let prev = match i {
        0 => return true,
        _ => bytes[i - 1],
    };
    if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' || prev == b'$' {
        return false;
    }
    !(prev == b':' && i >= 2 && bytes[i - 2] == b':')
}

/// Applies `f` to every stretch of `body` outside a `"…"` literal — prose is not source, and a sentence mentioning the theme is not a read of it.
fn replace_outside_strings(body: &str, f: impl Fn(&str) -> String) -> String {
    let bytes = body.as_bytes();
    let (mut out, mut chunk_at, mut i) = (String::with_capacity(body.len()), 0usize, 0usize);
    while i < body.len() {
        if let Some(end) = string_end(bytes, i) {
            out.push_str(&f(&body[chunk_at..i]));
            out.push_str(&body[i..end]);
            (chunk_at, i) = (end, end);
            continue;
        }
        i += 1;
    }
    out.push_str(&f(&body[chunk_at..]));
    out
}

/// A boxed closure in a `Props` declaration becomes the shape that says what it is for.
///
/// `Box<dyn Fn() -> T>` was how a prop said "a value that can change", and `Reactive<T>` is that now — an enum, so a literal costs no allocation and a signal converts straight into it. `Box<dyn Fn(…)>` with nothing returned is a *handler*, and becomes `Rc<dyn Fn(…)>`: a props struct is `Clone` now, and a unique box has no second owner to give.
///
/// Only inside the declaration. A `Box<dyn Fn…>` elsewhere in `[logic]` is the author's own, and a props struct is the one place the framework has an opinion about.
fn shared_handlers(body: &str) -> String {
    let Some(at) = body.find("pub struct Props {") else {
        return body.to_string();
    };
    let Some(end) = body[at..].find("\n}").map(|i| at + i) else {
        return body.to_string();
    };
    let (declaration, reactive) = rewrite_boxed_props(&body[at..end]);
    if declaration == body[at..end] {
        return body.to_string();
    }
    // A prop that was a closure is read with `.get()` now, not called. Only the ones this pass just changed, by name, so a closure the author keeps for its own sake is left alone.
    let mut rest = format!("{}{}", &declaration, &body[end..]);
    for name in &reactive {
        rest = rest.replace(&format!("(props.{name})()"), &format!("props.{name}.get()"));
    }
    let out = format!("{}{rest}", &body[..at]);
    match !out.contains("Rc<dyn Fn") || out.contains("use std::rc::Rc;") || out.contains("::rc::{")
    {
        true => out,
        false => format!("use std::rc::Rc;\n\n{out}"),
    }
}

fn rewrite_boxed_props(declaration: &str) -> (String, Vec<String>) {
    // A `#[props(default = Box::new(…))]` defaults a handler, and a handler is shared now.
    let declaration =
        &declaration.replace("#[props(default = Box::new(", "#[props(default = Rc::new(");
    let mut out = String::with_capacity(declaration.len());
    let mut reactive = Vec::new();
    let mut rest = declaration.as_str();
    // `Rc` as well as `Box`, because what decides is whether the closure returns something: a prop already moved to `Rc<dyn Fn() -> T>` by hand is still a value wearing a handler's shape.
    while let Some((at, owner)) = ["Box<dyn Fn", "Rc<dyn Fn"]
        .iter()
        .filter_map(|owner| rest.find(owner).map(|at| (at, *owner)))
        .min()
    {
        let head = owner.len() - "<dyn Fn".len();
        let Some(close) = closing_angle(rest, at + head) else {
            out.push_str(&rest[..at + head]);
            rest = &rest[at + head..];
            continue;
        };
        out.push_str(&rest[..at]);
        let inner = &rest[at + head + 1..close];
        // A closure that takes and returns is a callback the framework has no shape for, so it is left as written. `Fn() -> T` is the reactive one; `Fn(T)` is a handler.
        let reads = inner
            .split_once("->")
            .filter(|(args, _)| args.trim().ends_with("()"));
        match reads {
            // A value the prop reads, which is what `Reactive` is: `Const(T)` or a closure, one shape.
            Some((_, yields)) => {
                if let Some(name) = field_name(&out) {
                    reactive.push(name);
                }
                out.push_str(&format!("Reactive<{}>", yields.trim()));
                // The inline `= Box::new(…)` default is a closure too, and the type it defaults no longer takes one.
                if rest[close + 1..].starts_with(" = Box::new(") {
                    out.push_str(" = Reactive::of(");
                    rest = &rest[close + 1 + " = Box::new(".len()..];
                    continue;
                }
            }
            // A handler, shared so the struct can be cloned.
            None => out.push_str(&format!("Rc<{inner}>")),
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    (permissive(&out, &reactive), reactive)
}

/// The name of the field whose type is about to be written at the end of `so_far`.
fn field_name(so_far: &str) -> Option<String> {
    let line = so_far.rsplit('\n').next()?;
    let (name, _) = line.rsplit_once(':')?;
    Some(name.trim().trim_start_matches("pub ").to_string())
}

/// `#[props(into)]` on every prop that became a `Reactive`, which is what lets a call site keep writing the literal or the signal it always wrote instead of naming the wrapper.
fn permissive(declaration: &str, reactive: &[String]) -> String {
    let mut out = String::with_capacity(declaration.len());
    for line in declaration.split_inclusive('\n') {
        let names = line
            .split_once(':')
            .map(|(name, _)| name.trim().trim_start_matches("pub ").to_string());
        let wants = names.is_some_and(|name| reactive.contains(&name));
        if wants && !out.trim_end().ends_with(']') {
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            out.push_str(&format!("{indent}#[props(into)]\n"));
        }
        out.push_str(line);
    }
    out
}

/// The `>` closing the `<` at `open`, counting the ones between — and not the one in `->`, which is half an arrow and closes nothing.
fn closing_angle(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        match c {
            '<' => depth += 1,
            '>' if i == 0 || bytes[i - 1] != b'-' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `clip:x` → `clip:Clip::x()`. A clip is a shape now, not an axis from a closed set of three.
fn clip_shapes(body: &str) -> String {
    replace_outside_strings(body, |chunk| {
        chunk
            .replace("clip:x", "clip:Clip::x()")
            .replace("clip:y", "clip:Clip::y()")
            .replace("clip:both", "clip")
    })
}

/// Moves every `[style]` constant into `[logic]` as a `const`, and rewrites the names that referred to it. `[style]` keeps classes — named bundles of properties, which are reuse rather than a second evaluation model.
fn style_constants_to_logic(source: &str) -> String {
    let constants: Vec<(String, String, String)> = zones(source)
        .iter()
        .filter(|z| z.section == Section::Style)
        .flat_map(|z| z.body.lines())
        .filter_map(style_constant)
        .collect();
    if constants.is_empty() {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len());
    for zone in zones(source) {
        out.push_str(zone.header);
        match zone.section {
            Section::Logic => {
                out.push_str(zone.body.trim_end_matches('\n'));
                out.push_str("\n\n");
                for (name, ty, value) in &constants {
                    out.push_str(&format!("const {}: {ty} = {value};\n", name.to_uppercase()));
                }
                out.push('\n');
            }
            Section::Style => {
                for line in zone.body.split_inclusive('\n') {
                    if style_constant(line).is_none() {
                        out.push_str(line);
                    }
                }
            }
            _ => out.push_str(&replace_outside_strings(zone.body, |chunk| {
                let mut chunk = chunk.to_string();
                for (name, _, _) in &constants {
                    chunk = replace_whole_name(&chunk, name, &name.to_uppercase());
                }
                chunk
            })),
        }
    }
    out
}

/// `(name, Rust type, Rust value)` for a `[style]` constant line, or `None` for a class header, a class property (indented) or a blank.
fn style_constant(line: &str) -> Option<(String, String, String)> {
    if line.starts_with([' ', '\t']) || line.trim().is_empty() || line.trim_start().starts_with('@')
    {
        return None;
    }
    let (name, value) = line.split_once(':')?;
    let (name, value) = (name.trim(), value.trim());
    if name.is_empty() || value.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }
    let (ty, rust) = match value.strip_prefix('#') {
        Some(hex) => ("Color".to_string(), hex_to_color(hex)?),
        None => match value.parse::<f32>() {
            Ok(n) => ("f32".to_string(), format!("{n:?}")),
            Err(_) => ("&str".to_string(), format!("{value:?}")),
        },
    };
    Some((name.to_string(), ty, rust))
}

fn hex_to_color(hex: &str) -> Option<String> {
    let expand = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    let chars: Vec<char> = hex.chars().collect();
    let [r, g, b, a] = match chars.len() {
        3 => [expand(chars[0])?, expand(chars[1])?, expand(chars[2])?, 255],
        4 => [
            expand(chars[0])?,
            expand(chars[1])?,
            expand(chars[2])?,
            expand(chars[3])?,
        ],
        6 => [byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?, 255],
        8 => [
            byte(&hex[0..2])?,
            byte(&hex[2..4])?,
            byte(&hex[4..6])?,
            byte(&hex[6..8])?,
        ],
        _ => return None,
    };
    let f = |c: u8| format!("{:.3}", c as f32 / 255.0);
    Some(format!(
        "Color::rgba({}, {}, {}, {})",
        f(r),
        f(g),
        f(b),
        f(a)
    ))
}

/// `from` → `to` wherever `from` is a whole name in a *value* position. A name followed by `:` is the attribute key, and a constant named `radius` used under a key of the same name is what makes that distinction load-bearing.
fn replace_whole_name(chunk: &str, from: &str, to: &str) -> String {
    let bytes = chunk.as_bytes();
    let (mut out, mut i) = (String::with_capacity(chunk.len()), 0usize);
    while i < chunk.len() {
        let ends_ok = bytes
            .get(i + from.len())
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_' || *b == b':'));
        if chunk[i..].starts_with(from) && starts_a_name(bytes, i) && ends_ok {
            out.push_str(to);
            i += from.len();
            continue;
        }
        let ch = chunk[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `stem -> crate::path::to::stem` for every `.rsx` in the sweep, so a tag that used to resolve through the crate-root re-export can be given the `use` line it now needs.
fn component_modules(sources: &[PathBuf]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for path in sources {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(src_at) = path.components().position(|c| c.as_os_str() == "src") else {
            continue;
        };
        let segments: Vec<String> = path
            .with_extension("")
            .components()
            .skip(src_at + 1)
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        out.insert(stem.to_string(), format!("crate::{}", segments.join("::")));
    }
    out
}

/// Adds a `use` line to `[logic]` for every component tag the file calls and does not already import. Each `.rsx` is a module now, so a caller imports what it uses instead of reaching a crate-root re-export.
fn imports_for_tags(source: &str, modules: &BTreeMap<String, String>, own: &str) -> String {
    let mut wanted: Vec<&str> = Vec::new();
    for zone in zones(source) {
        if !matches!(zone.section, Section::View | Section::Preview) {
            continue;
        }
        for line in zone.body.lines() {
            let Some(tag) = leading_tag(line) else {
                continue;
            };
            if tag != own && modules.contains_key(tag) && !wanted.contains(&tag) {
                wanted.push(tag);
            }
        }
    }
    let missing: Vec<&&str> = wanted
        .iter()
        .filter(|tag| !source.contains(&format!("::{tag}::")))
        .collect();
    if missing.is_empty() {
        return source.to_string();
    }

    let lines: Vec<String> = missing
        .iter()
        .map(|tag| {
            let path = &modules[**tag];
            format!("use {path}::{{{tag}, {}Props}};\n", pascal(tag))
        })
        .collect();

    let mut out = String::with_capacity(source.len());
    let mut placed = false;
    for zone in zones(source) {
        out.push_str(zone.header);
        if zone.section == Section::Logic && !placed {
            placed = true;
            for line in &lines {
                out.push_str(line);
            }
        }
        out.push_str(zone.body);
    }
    match placed {
        true => out,
        // A file with no `[logic]` at all gets one, since a `use` has nowhere else to live.
        false => format!("[logic]\n{}\n{out}", lines.concat()),
    }
}

fn leading_tag(line: &str) -> Option<&str> {
    let tag = leading_token(line)?;
    let ok = tag
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (ok && !is_control_flow(tag)).then_some(tag)
}

/// The first word of a line, skipping its indent. `None` for a blank line or a comment.
fn leading_token(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }
    let token = trimmed.split([' ', '\t', '(', ':']).next()?;
    (!token.is_empty()).then_some(token)
}

fn pascal(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The `build "…"` and `widget "…"` sites, reported rather than guessed: turning `build "tray_icon(item, config, fg, size)?"` into a tag needs *names* for four positional arguments, and only a person knows them.
fn escapes_needing_a_person(path: &Path, source: &str) -> Vec<(PathBuf, usize, String)> {
    let mut out = Vec::new();
    for zone in zones(source) {
        if !matches!(zone.section, Section::View | Section::Preview) {
            continue;
        }
        let offset = source.len() - zone.body.len();
        let first_line = source[..offset].lines().count();
        for (i, line) in zone.body.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("build \"") || trimmed.starts_with("widget \"") {
                out.push((path.to_path_buf(), first_line + i + 1, trimmed.to_string()));
            }
        }
    }
    out
}

fn collect_rsx(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        if root.extension().and_then(|e| e.to_str()) == Some("rsx") {
            out.push(root.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        collect_rsx(&path, out);
    }
}

/// The component this file *is*, which it never imports: a `[preview]` calling it is a sibling function in the same generated module.
fn own_stem(path: &Path) -> &str {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
}

fn display(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated(source: &str) -> String {
        migrate(source, &BTreeMap::new(), "demo", &BTreeMap::new())
    }

    #[test]
    fn a_value_loses_its_parens_and_a_directive_keeps_them() {
        let out = migrated(
            "[view]\nbtn label(\"Save\") gap(8) on_press(|| f()) transition(fill 250ms ease-out)\n",
        );
        assert_eq!(
            out,
            "[view]\nbtn label:\"Save\" gap:8 on_press:(|| f()) transition(fill 250ms ease-out)\n"
        );
    }

    /// A `.rsx` line holds prose, and a codemod that walks it byte by byte splits an em dash in three.
    #[test]
    fn a_line_of_prose_survives_the_walk() {
        let source = "[view]\n// Un guion largo — y un acento: canción\ncol gap(8)\n";
        assert_eq!(
            migrated(source),
            "[view]\n// Un guion largo — y un acento: canción\ncol gap:8\n"
        );
    }

    /// A control-flow line is Rust: `if shown($seen)` is a call, `for … in options()` is a call, and a `[view]`-level `let` holds one. None of the three has an attribute in it.
    #[test]
    fn a_control_flow_line_is_left_alone() {
        let source = "[view]\nif shown($seen)\n    let over = signal(false)\n    for (i, x) in options()\n        row gap(4)\n";
        assert_eq!(
            migrated(source),
            "[view]\nif shown($seen)\n    let over = signal(false)\n    for (i, x) in options()\n        row gap:4\n"
        );
    }

    /// A leading `::` against the colon reads as `key::…`, which is a key nobody wrote.
    #[test]
    fn a_value_that_opens_with_a_path_separator_is_parenthesised() {
        assert_eq!(
            migrated("[view]\nrow gap(::ui::scale::space::md())\n"),
            "[view]\nrow gap:(::ui::scale::space::md())\n"
        );
    }

    #[test]
    fn a_call_inside_a_value_is_not_an_attribute() {
        let out = migrated("[view]\ncol gap(space::lg()) pad(scale(2, 3))\n");
        assert_eq!(out, "[view]\ncol gap:space::lg() pad:scale(2, 3)\n");
    }

    #[test]
    fn a_catalog_key_in_a_value_becomes_the_macro_and_content_keeps_the_literal() {
        let out = migrated("[view]\nbtn label:t\"buttons.save\"\ntext t\"nav.title\"\n");
        assert_eq!(
            out,
            "[view]\nbtn label:t!(\"buttons.save\")\ntext t\"nav.title\"\n"
        );
    }

    #[test]
    fn a_theme_read_gains_the_sigil_and_prose_does_not() {
        let out = migrated(
            "[logic]\nlet c = theme().primary;\n\n[view]\nbox fill:theme.surface\n    text \"switch the theme.\" color:theme.ink\n",
        );
        assert_eq!(
            out,
            "[logic]\nlet c = theme.get().primary;\n\n[view]\nbox fill:$theme.surface\n    text \"switch the theme.\" color:$theme.ink\n"
        );
    }

    /// A qualified call still names the crate's own accessor — which is what a nested `fn` inside `[logic]` needs, since it cannot see the view's binding.
    #[test]
    fn a_qualified_theme_call_is_not_the_views_binding() {
        let source = "[logic]\nfn draw() {\n    let t = crate::core::theme::theme();\n}\n";
        assert_eq!(migrated(source), source);
    }

    /// A props struct is `Clone` now, so a unique box in one is a struct that cannot reach a region that rebuilds. Only the declaration is rewritten — a `Box<dyn Fn>` elsewhere in `[logic]` is the author's own.
    #[test]
    fn a_handler_prop_becomes_a_shared_one() {
        let out = migrated(
            "[logic]\npub struct Props {\n    pub act: Box<dyn Fn()>,\n    pub label: Box<dyn Fn() -> String>,\n    pub tint: Option<Box<dyn Fn() -> Color>>,\n}\n\nlet held: Box<dyn Fn()> = Box::new(|| {});\n\n[view]\ncol\n",
        );
        assert!(out.starts_with("[logic]\nuse std::rc::Rc;\n"), "{out}");
        assert!(out.contains("pub act: Rc<dyn Fn()>,"), "{out}");
        assert!(
            out.contains("pub label: Reactive<String>,"),
            "a value, not a handler: {out}"
        );
        assert!(out.contains("pub tint: Option<Reactive<Color>>,"), "{out}");
        // A closure that takes and returns is a callback with no framework shape: left as written, since reading it as a value would drop the argument it is handed.
        let callback = migrated(
            "[logic]\npub struct Props {\n    pub style: Box<dyn Fn(RectStyle) -> RectStyle>,\n}\n\n[view]\ncol\n",
        );
        assert!(
            callback.contains("pub style: Rc<dyn Fn(RectStyle) -> RectStyle>,"),
            "{callback}"
        );

        // A prop already moved to `Rc` by hand is still a value if it returns one and a handler if it does not, which is what makes running the codemod twice safe.
        let again = migrated(&out);
        assert_eq!(again, out, "the rewrite is its own fixed point");
        let by_hand = migrated(
            "[logic]\nuse std::rc::Rc;\n\npub struct Props {\n    pub label: Rc<dyn Fn() -> String>,\n    pub act: Rc<dyn Fn()>,\n}\n\n[view]\ncol\n",
        );
        assert!(
            by_hand.contains("pub label: Reactive<String>,"),
            "{by_hand}"
        );
        assert!(by_hand.contains("pub act: Rc<dyn Fn()>,"), "{by_hand}");
        assert!(
            out.contains("let held: Box<dyn Fn()> = Box::new(|| {});"),
            "a binding outside the declaration is left alone: {out}"
        );
    }

    /// The inline `= Box::new(…)` default is a closure too, and the type it defaults no longer takes one.
    #[test]
    fn a_boxed_default_follows_the_type_it_defaults() {
        let out = migrated(
            "[logic]\npub struct Props {\n    pub text: Box<dyn Fn() -> String> = Box::new(String::new),\n}\n\n[view]\ncol\n",
        );
        assert!(
            out.contains("pub text: Reactive<String> = Reactive::of(String::new),"),
            "{out}"
        );

        // And the attribute form, which defaults a handler.
        let handler = migrated(
            "[logic]\npub struct Props {\n    #[props(default = Box::new(|_| {}))]\n    pub on_pick: Box<dyn Fn(bool)>,\n}\n\n[view]\ncol\n",
        );
        assert!(
            handler.contains("#[props(default = Rc::new(|_| {}))]"),
            "{handler}"
        );
        assert!(
            handler.contains("pub on_pick: Rc<dyn Fn(bool)>,"),
            "{handler}"
        );
    }

    /// The binding that shadows the view's handle is a top-level one. A `let theme = use_theme::<T>()` inside a nested `fn` is that function's own and shadows nothing in the view.
    #[test]
    fn a_theme_bound_inside_a_fn_shadows_nothing() {
        let out = migrated(
            "[logic]\nfn tint() -> Color {\n    let theme = use_theme::<NordTheme>();\n    theme.muted\n}\n\n[view]\ntext \"x\" font_size:theme.body\n",
        );
        assert!(out.contains("font_size:$theme.body"), "{out}");
        assert!(
            out.contains("    let theme = use_theme::<NordTheme>();"),
            "the nested binding is left alone: {out}"
        );
    }

    /// A file that binds `theme` itself means its own binding, not the view's handle — `$theme.base` on a `NordTheme` is a `.get()` the type does not have.
    #[test]
    fn a_file_that_binds_theme_keeps_meaning_its_own() {
        let source =
            "[logic]\nlet theme = use_theme::<NordTheme>();\n\n[view]\nbox fill:theme.base\n";
        assert_eq!(migrated(source), source);
    }

    /// A closure was how a call site said "a value that changes", and it fitted the `Box<dyn Fn() -> T>` the prop used to be. Only the props this sweep rewrote are wrapped; any other closure is a handler.
    #[test]
    fn a_closure_on_a_rewritten_prop_becomes_a_reactive() {
        let mut reactive = BTreeMap::new();
        reactive.insert("icon_glyph".to_string(), vec!["name".to_string()]);
        let out = migrate(
            "[view]\ncol\n    icon_glyph name:(|| \"cpu\".to_string()) on_press:(|| pick())\n",
            &BTreeMap::new(),
            "demo",
            &reactive,
        );
        assert!(
            out.contains("name:(Reactive::of(|| \"cpu\".to_string()))"),
            "{out}"
        );
        assert!(
            out.contains("on_press:(|| pick())"),
            "a handler stays one: {out}"
        );
    }

    #[test]
    fn a_clip_axis_becomes_the_shape_it_named() {
        let out = migrated("[view]\nrow clip:x\ncol clip:y\nbox clip\n");
        assert_eq!(
            out,
            "[view]\nrow clip:Clip::x()\ncol clip:Clip::y()\nbox clip\n"
        );
    }

    #[test]
    fn a_style_constant_moves_to_logic_and_takes_its_uses_with_it() {
        let out = migrated(
            "[logic]\nlet n = 1;\n\n[style]\nprimary: #4361ee\nradius: 6\n\n@card\n    width: 240\n\n[view]\nbox fill:primary radius:radius\n",
        );
        assert!(
            out.contains("const PRIMARY: Color = Color::rgba(0.263, 0.380, 0.933, 1.000);"),
            "{out}"
        );
        assert!(out.contains("const RADIUS: f32 = 6.0;"), "{out}");
        assert!(out.contains("box fill:PRIMARY radius:RADIUS"), "{out}");
        assert!(
            out.contains("@card\n    width: 240"),
            "a class stays: {out}"
        );
        assert!(!out.contains("primary: #4361ee"), "{out}");
    }

    #[test]
    fn a_file_already_in_the_new_grammar_comes_out_unchanged() {
        let source = "[logic]\nlet n = 1;\n\n[style]\n@card\n    width: 240\n\n[view]\nbox @card fill:$theme.surface clip:Clip::x()\n    btn label:\"Save\" on_press:(|| f())\n";
        assert_eq!(migrated(source), source);
    }

    #[test]
    fn a_component_tag_gains_the_use_line_the_crate_root_used_to_supply() {
        let mut modules = BTreeMap::new();
        modules.insert("card".to_string(), "crate::ui::card".to_string());
        let out = migrate(
            "[logic]\nlet n = 1;\n\n[view]\ncol\n    card gap:8\n",
            &modules,
            "demo",
            &BTreeMap::new(),
        );
        assert!(
            out.starts_with("[logic]\nuse crate::ui::card::{card, CardProps};\nlet n = 1;"),
            "{out}"
        );
    }

    /// A `[preview]` in a component's own file calls it as a sibling function, so importing it would be a module importing itself.
    #[test]
    fn a_file_never_imports_the_component_it_is() {
        let mut modules = BTreeMap::new();
        modules.insert("stat".to_string(), "crate::ui::stat".to_string());
        let source =
            "[logic]\nlet n = 1;\n\n[view]\ncol\n\n[preview \"Stat\"]\nstat value:\"60\"\n";
        assert_eq!(migrate(source, &modules, "stat", &BTreeMap::new()), source);
    }

    #[test]
    fn an_escape_that_needs_names_is_reported_rather_than_guessed() {
        let found = escapes_needing_a_person(
            Path::new("a.rsx"),
            "[logic]\nlet x = 1;\n\n[view]\ncol\n    build \"tray(item, cfg)?\"\n    widget \"icon\"\n",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].1, 6);
        assert!(found[0].2.starts_with("build \"tray("));
        assert_eq!(found[1].1, 7);
    }
}
