//! What `telar::preview::preview!` records of where a Rust preview is written: its body's source text as written, rather than as `stringify!` prints it, and the file it is in.

use std::path::{Path, PathBuf};

use proc_macro::{Delimiter, Span, TokenStream, TokenTree};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// The body's text as written, or `None` where it has no source of its own, as when another macro wrote it.
pub fn written(body: TokenStream) -> Option<String> {
    let tokens = unwrapped(body);
    let text = match tokens.as_slice() {
        [] => return None,
        [only] => only.span().source_text()?,
        [first, .., last] => between(first.span(), last.span())?,
    };
    Some(dedent(&text))
}

/// The tokens inside the invisible groups a macro fragment arrives wrapped in, whose span is the fragment in the macro rather than what the author wrote.
fn unwrapped(tokens: TokenStream) -> Vec<TokenTree> {
    let mut tokens: Vec<TokenTree> = tokens.into_iter().collect();
    while let [TokenTree::Group(group)] = tokens.as_slice()
        && group.delimiter() == Delimiter::None
    {
        tokens = group.stream().into_iter().collect();
    }
    tokens
}

/// The text from the start of `first` to the end of `last`, read from the file both are in: `Span::join` is not stable.
fn between(first: Span, last: Span) -> Option<String> {
    first.source_text()?;
    last.source_text()?;
    let file = first.local_file()?;
    if last.local_file()? != file {
        return None;
    }
    let source = std::fs::read_to_string(file).ok()?;
    let start = offset(&source, first.start().line(), first.start().column())?;
    let end = offset(&source, last.end().line(), last.end().column())?;
    source.get(start..end).map(str::to_string)
}

/// The byte offset of a 1-based line and a 1-based column counted in characters, as a span reports them.
pub fn offset(source: &str, line: usize, column: usize) -> Option<usize> {
    let line_start = match line {
        0 => return None,
        1 => 0,
        _ => source.match_indices('\n').nth(line - 2)?.0 + 1,
    };
    let rest = &source[line_start..];
    let within = rest
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(rest.len()))
        .nth(column.checked_sub(1)?)?;
    Some(line_start + within)
}

/// `text` as it reads on its own: the lines after the first lose the indentation they all share, since the first starts wherever the body did on its line.
pub fn dedent(text: &str) -> String {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let rest: Vec<&str> = lines.collect();
    let mut out = first.trim_end().to_string();
    for line in crate::text::dedent(&rest) {
        out.push('\n');
        out.push_str(line);
    }
    out
}

/// The expression for the absolute path of the file `tokens` are written in, as a `.rsx` preview records its own: `CARGO_MANIFEST_DIR` joined to the path within the package, or the absolute path itself for a file outside it, or `""` when the tokens come from no file.
pub fn file_of(tokens: TokenStream) -> TokenStream2 {
    let Some(file) = unwrapped(tokens)
        .first()
        .and_then(|token| token.span().local_file())
    else {
        return quote! { "" };
    };
    // rustc reports a path relative to the directory cargo started it in, which is also the macro's.
    let absolute = std::path::absolute(&file).unwrap_or(file);
    match within_package(&absolute) {
        Some(relative) => telar_project::naming::preview_file_expr(&relative)
            .parse()
            .expect("a path joined to the manifest dir is an expression"),
        None => {
            let text = absolute.to_string_lossy();
            quote! { #text }
        }
    }
}

/// `file`'s path within the package being compiled, with `/` between its parts.
fn within_package(file: &Path) -> Option<String> {
    let package = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    let relative = file.strip_prefix(&package).ok()?;
    let parts: Option<Vec<&str>> = relative.iter().map(|part| part.to_str()).collect();
    Some(parts?.join("/"))
}

#[cfg(test)]
#[path = "preview_source_test.rs"]
mod tests;
