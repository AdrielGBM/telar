//! The faces `[[telar.fonts]]` declares, as a browser loads them: hashed files, an `@font-face` per file, and a preload per file so none waits for CSS to ask.

use std::collections::BTreeMap;
use std::path::Path;

use telar_project::FontDeclaration;

use super::assets::Assets;
use super::media::media_type;
use super::page::{HeadTag, Page};

/// What a preload carries to name the family its face answers to, which is how a canvas build finds the faces to fetch into its shaper. Read by `renderer_web::FAMILY_ATTRIBUTE`, which must spell it the same.
pub(crate) const FAMILY_ATTRIBUTE: &str = "data-telar-family";

/// Where the faces go in the output, under their own names before hashing.
const FONTS_DIR: &str = "fonts";

/// Emits each declared face as a hashed file and writes its `@font-face` rule and preload into `page`.
///
/// Two entries naming one file share it — a variable face declared once per style is one download. Two different files with one name are refused rather than renamed, so a name in the network panel always says which file it is.
pub(crate) fn declare_fonts(
    page: &mut Page,
    assets: &mut Assets,
    package_root: &Path,
    fonts: &[FontDeclaration],
) -> Result<(), String> {
    let mut emitted: BTreeMap<String, (String, String)> = BTreeMap::new();
    for font in fonts {
        let path = font.path(package_root);
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| format!("the font {:?} names no file", font.family))?;
        let logical = format!("{FONTS_DIR}/{file_name}");
        let hashed = match emitted.get(&logical) {
            Some((src, hashed)) if *src == font.src => hashed.clone(),
            Some((src, _)) => {
                return Err(format!(
                    "the fonts `{src}` and `{}` share the file name `{file_name}`; rename one",
                    font.src
                ));
            }
            None => {
                let bytes = std::fs::read(&path).map_err(|e| {
                    format!(
                        "could not read the font {:?} at {}: {e}",
                        font.family,
                        path.display()
                    )
                })?;
                let hashed = assets.emit(&logical, &bytes)?;
                page.font_preloads.push(
                    HeadTag::font_preload(page.url(&hashed), media_type(&path))
                        .attr(FAMILY_ATTRIBUTE, font.family.clone()),
                );
                emitted.insert(logical, (font.src.clone(), hashed.clone()));
                hashed
            }
        };
        page.font_faces
            .push_str(&font_face(font, &page.url(&hashed)));
    }
    Ok(())
}

/// One `@font-face` rule.
pub(crate) fn font_face(font: &FontDeclaration, url: &str) -> String {
    let (min, max) = font.weight_range();
    let weight = match min == max {
        true => min.to_string(),
        false => format!("{min} {max}"),
    };
    let mut rule = format!(
        "@font-face {{\n  font-family: {};\n  src: url({}) format(\"{}\");\n  font-weight: {weight};\n  font-style: {};\n",
        css_string(&font.family),
        css_string(url),
        font.format().css_name(),
        font.style.as_str(),
    );
    if let Some((low, high)) = font.stretch_range() {
        rule.push_str(&format!("  font-stretch: {low}% {high}%;\n"));
    }
    rule.push_str(&format!("  font-display: {};\n", font.display.as_str()));
    if let Some(factor) = font.size_adjust {
        rule.push_str(&format!("  size-adjust: {}%;\n", round_percent(factor)));
    }
    rule.push_str("}\n");
    rule
}

fn round_percent(factor: f32) -> f32 {
    (factor * 10_000.0).round() / 100.0
}

/// A CSS string literal. `<` is escaped too: the rules sit inside a `<style>`, which a `</style>` in a family name would close.
fn css_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '<' => quoted.push_str("\\3c "),
            '\n' => quoted.push_str("\\a "),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
#[path = "fonts_test.rs"]
mod tests;
