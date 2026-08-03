//! Parses the `transition:` attribute value into per-property animation curves for the `motion` engine.
//!
//! Syntax (see `docs/animations.md` → Design → `.rsx` transition syntax): `transition:<prop> <duration> [<easing>|spring(k,c)]`, with several properties separated by commas. `<duration>` is `200ms`/`0.3s`; easing keywords are `linear|ease-in|ease-out|ease-in-out` or `cubic-bezier(a,b,c,d)`; the easing defaults to `ease-out` when omitted; a `spring(stiffness, damping)` replaces the duration+easing entirely.

use crate::style::format_f32;

/// One `transition:<prop> …` clause resolved to codegen strings.
pub(crate) struct TransitionSpec {
    pub prop: String,
    /// The `motion::` curve expression, e.g. `motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut)` or `motion::spring(170.0, 26.0)`.
    pub curve: String,
}

/// Properties a `transition:` can animate (F5 in `docs/animations.md`).
///
/// Paint and transform, and deliberately not the layout box. Both halves are read per frame from a closure the
/// renderer already re-runs, so animating them costs a repaint and nothing else — the "no relayout" invariant
/// (F5 in `docs/animations.md`) that the whole design rests on. Animating `width`/`height`/`x`/`y` would put a
/// layout pass in every frame of every transition, which is a different decision and needs its own.
///
/// A transform is enough for the shape this was missing: an indicator that slides to the active item moves by
/// `translate_x`, not by its box.
const SUPPORTED_PROPS: &[&str] = &[
    "opacity",
    "fill",
    "stroke",
    "color",
    "rotate",
    "scale",
    "scale_x",
    "scale_y",
    "translate_x",
    "translate_y",
];

/// Parses a `transition:` value into resolved specs plus human-readable error messages (a malformed clause becomes an error but does not abort the others). Parentheses are respected so the commas inside `cubic-bezier(...)`/`spring(...)` never split clauses.
pub(crate) fn parse_transition_value(value: &str) -> (Vec<TransitionSpec>, Vec<String>) {
    let mut specs = Vec::new();
    let mut errors = Vec::new();
    for group in split_top_level(value, ',') {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }
        match parse_clause(group) {
            Ok(spec) => specs.push(spec),
            Err(e) => errors.push(e),
        }
    }
    (specs, errors)
}

fn parse_clause(clause: &str) -> Result<TransitionSpec, String> {
    let tokens = split_top_level_ws(clause);
    let prop = tokens
        .first()
        .ok_or_else(|| "transition: empty clause".to_string())?
        .to_string();
    if !SUPPORTED_PROPS.contains(&prop.as_str()) {
        return Err(format!(
            "transition: unsupported property `{prop}` (supported: {})",
            SUPPORTED_PROPS.join(", ")
        ));
    }
    let second = tokens.get(1).ok_or_else(|| {
        format!(
            "transition:{prop} needs a duration (e.g. `200ms`) or a `spring(stiffness, damping)`"
        )
    })?;

    if let Some(inner) = second
        .strip_prefix("spring(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let curve = parse_spring(inner).map_err(|e| format!("transition:{prop} {e}"))?;
        return Ok(TransitionSpec { prop, curve });
    }

    let duration = parse_duration(second).map_err(|e| format!("transition:{prop} {e}"))?;
    let easing = match tokens.get(2) {
        Some(e) => parse_easing(e).map_err(|m| format!("transition:{prop} {m}"))?,
        None => "motion::Easing::EaseOut".to_string(),
    };
    Ok(TransitionSpec {
        prop,
        curve: format!("motion::tween({duration}, {easing})"),
    })
}

/// Parses a duration token (`200ms`, `0.3s`) into a `std::time::Duration` expression. `ms` is checked before `s` because `ms` also ends in `s`.
fn parse_duration(tok: &str) -> Result<String, String> {
    if let Some(ms) = tok.strip_suffix("ms") {
        let n: f32 = ms.trim().parse().map_err(|_| {
            format!("has an invalid duration `{tok}` (expected e.g. `200ms` or `0.3s`)")
        })?;
        return Ok(format!(
            "std::time::Duration::from_millis({})",
            n.round() as u64
        ));
    }
    if let Some(s) = tok.strip_suffix('s') {
        let n: f32 = s.trim().parse().map_err(|_| {
            format!("has an invalid duration `{tok}` (expected e.g. `200ms` or `0.3s`)")
        })?;
        return Ok(format!(
            "std::time::Duration::from_millis({})",
            (n * 1000.0).round() as u64
        ));
    }
    Err(format!(
        "has an invalid duration `{tok}` (expected e.g. `200ms` or `0.3s`)"
    ))
}

fn parse_easing(tok: &str) -> Result<String, String> {
    Ok(match tok {
        "linear" => "motion::Easing::Linear".to_string(),
        "ease-in" => "motion::Easing::EaseIn".to_string(),
        "ease-out" => "motion::Easing::EaseOut".to_string(),
        "ease-in-out" => "motion::Easing::EaseInOut".to_string(),
        _ => {
            let inner = tok
                .strip_prefix("cubic-bezier(")
                .and_then(|s| s.strip_suffix(')'))
                .ok_or_else(|| {
                    format!(
                        "has an unknown easing `{tok}` (expected linear|ease-in|ease-out|ease-in-out|cubic-bezier(...)|spring(...))"
                    )
                })?;
            let nums = parse_f32_list(inner)?;
            if nums.len() != 4 {
                return Err("has a malformed `cubic-bezier(...)` (expected 4 numbers)".to_string());
            }
            format!(
                "motion::Easing::CubicBezier({}, {}, {}, {})",
                nums[0], nums[1], nums[2], nums[3]
            )
        }
    })
}

fn parse_spring(inner: &str) -> Result<String, String> {
    let nums = parse_f32_list(inner)?;
    if nums.len() != 2 {
        return Err(
            "has a malformed `spring(...)` (expected `spring(stiffness, damping)`)".to_string(),
        );
    }
    Ok(format!("motion::spring({}, {})", nums[0], nums[1]))
}

/// Parses a comma-separated list of f32 literals, each formatted with a decimal point (so `1` becomes `1.0`).
fn parse_f32_list(s: &str) -> Result<Vec<String>, String> {
    s.split(',')
        .map(|p| {
            let p = p.trim();
            p.parse::<f32>()
                .map(format_f32)
                .map_err(|_| format!("has a non-numeric value `{p}`"))
        })
        .collect()
}

/// Paren-depth-aware splitter shared by [`split_top_level`] and [`split_top_level_ws`]: a boundary char only splits at depth 0, so separators nested inside `(...)` (e.g. cubic-bezier/spring args) stay part of the current segment; `keep_empty` controls whether empty segments (including a trailing one) are dropped.
fn split_top_level_by(
    s: &str,
    is_boundary: impl Fn(char) -> bool,
    keep_empty: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if is_boundary(c) && depth == 0 => {
                if keep_empty || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if keep_empty || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Splits `s` on `sep`, ignoring separators nested inside parentheses.
fn split_top_level(s: &str, sep: char) -> Vec<String> {
    split_top_level_by(s, |c| c == sep, true)
}

/// Splits `s` on whitespace runs, ignoring whitespace nested inside parentheses (so `cubic-bezier(0.4, 0, 0.2, 1)` stays one token).
fn split_top_level_ws(s: &str) -> Vec<String> {
    split_top_level_by(s, char::is_whitespace, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_with_keyword_easing() {
        let (specs, errors) = parse_transition_value("opacity 200ms ease-out");
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].prop, "opacity");
        assert_eq!(
            specs[0].curve,
            "motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut)"
        );
    }

    #[test]
    fn easing_defaults_to_ease_out_when_omitted() {
        let (specs, errors) = parse_transition_value("fill 150ms");
        assert!(errors.is_empty());
        assert_eq!(
            specs[0].curve,
            "motion::tween(std::time::Duration::from_millis(150), motion::Easing::EaseOut)"
        );
    }

    #[test]
    fn seconds_duration_becomes_millis() {
        let (specs, _) = parse_transition_value("opacity 0.3s linear");
        assert_eq!(
            specs[0].curve,
            "motion::tween(std::time::Duration::from_millis(300), motion::Easing::Linear)"
        );
    }

    #[test]
    fn cubic_bezier_and_spring() {
        let (cb, _) = parse_transition_value("fill 150ms cubic-bezier(0.4,0,0.2,1)");
        assert_eq!(
            cb[0].curve,
            "motion::tween(std::time::Duration::from_millis(150), motion::Easing::CubicBezier(0.4, 0.0, 0.2, 1.0))"
        );
        let (sp, _) = parse_transition_value("fill spring(170,26)");
        assert_eq!(sp[0].curve, "motion::spring(170.0, 26.0)");
    }

    #[test]
    fn comma_separated_clauses_respect_parentheses() {
        let (specs, errors) =
            parse_transition_value("opacity 200ms cubic-bezier(0.4,0,0.2,1), fill 150ms linear");
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].prop, "opacity");
        assert_eq!(specs[1].prop, "fill");
    }

    #[test]
    fn unsupported_property_and_bad_duration_report_errors() {
        let (specs, errors) = parse_transition_value("radius 200ms");
        assert!(specs.is_empty());
        assert!(errors[0].contains("unsupported property `radius`"));

        let (_, errors) = parse_transition_value("opacity 200");
        assert!(errors[0].contains("invalid duration"));
    }
}
