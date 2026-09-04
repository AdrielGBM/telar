//! What a prop that changes turns into: a `Reactive` at the call site, a shared handler, and the declaration behind both.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::imports::leading_tag;
use super::own_stem;
use super::text::{closing_angle, closing_paren};
/// `tag key:(|| …)` → `tag key:(Reactive::of(|| …))` for the props this sweep turned into a `Reactive`.
///
/// A closure was how a call site said "a value that changes", and it fitted the `Box<dyn Fn() -> T>` the prop used to be. `Reactive<T>` cannot take one through `Into` — the blanket `From<T>` already claims every type — so the wrapper is spelled out. Only for props this pass rewrote, by tag and by name: any other closure is a handler and stays one.
pub(super) fn reactive_closures(body: &str, reactive: &BTreeMap<String, Vec<String>>) -> String {
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
pub(super) fn reactive_props(sources: &[PathBuf]) -> BTreeMap<String, Vec<String>> {
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

/// A boxed closure in a `Props` declaration becomes the shape that says what it is for.
///
/// `Box<dyn Fn() -> T>` was how a prop said "a value that can change", and `Reactive<T>` is that now — an enum, so a literal costs no allocation and a signal converts straight into it. `Box<dyn Fn(…)>` with nothing returned is a *handler*, and becomes `Rc<dyn Fn(…)>`: a props struct is `Clone` now, and a unique box has no second owner to give.
///
/// Only inside the declaration. A `Box<dyn Fn…>` elsewhere in `[logic]` is the author's own, and a props struct is the one place the framework has an opinion about.
pub(super) fn shared_handlers(body: &str) -> String {
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

pub(super) fn rewrite_boxed_props(declaration: &str) -> (String, Vec<String>) {
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
pub(super) fn field_name(so_far: &str) -> Option<String> {
    let line = so_far.rsplit('\n').next()?;
    let (name, _) = line.rsplit_once(':')?;
    Some(name.trim().trim_start_matches("pub ").to_string())
}

/// `#[props(into)]` on every prop that became a `Reactive`, which is what lets a call site keep writing the literal or the signal it always wrote instead of naming the wrapper.
pub(super) fn permissive(declaration: &str, reactive: &[String]) -> String {
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
