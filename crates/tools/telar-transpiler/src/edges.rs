//! The per-edge halves of the two box properties that carry one number per edge: `stroke_width` across the four sides, and `radius` across the four corners.
//!
//! One module for both, because an author asking for a rule under a header and an author asking for a panel rounded only at the top are asking the same question in the same grammar — a CSS shorthand, or a name per edge. Only which edge a name lands on differs, which is the one thing each caller passes in.

use telar_parser::Attr;

/// What an edge whose value the key cannot mean contributes, once `value_kind` has reported it on the attribute: the build stops before any of these edges reach a `BorderRadius`.
const ZERO: &str = "0.0";

/// Where a suffixed attribute puts its value.
pub enum EdgeTarget {
    /// Indices into the property's own four slots.
    Slots(&'static [usize]),
    /// The edge the text comes from, resolved against the writing direction at paint time.
    Start,
    /// The edge the text runs towards.
    End,
}

impl EdgeTarget {
    /// How specifically this name picks its edges. A name for one edge outranks a name for a pair, which outranks the shorthand — so `radius:8 radius_top:0` is the same shape however the author ordered it.
    fn specificity(&self) -> u8 {
        match self {
            EdgeTarget::Slots(s) => 4 - s.len() as u8,
            EdgeTarget::Start | EdgeTarget::End => u8::MAX,
        }
    }
}

/// The suffixes of `stroke_*`, over sides ordered `[top, right, bottom, left]`.
pub fn side_target(suffix: &str) -> Option<EdgeTarget> {
    Some(match suffix {
        "top" => EdgeTarget::Slots(&[0]),
        "right" => EdgeTarget::Slots(&[1]),
        "bottom" => EdgeTarget::Slots(&[2]),
        "left" => EdgeTarget::Slots(&[3]),
        "x" => EdgeTarget::Slots(&[1, 3]),
        "y" => EdgeTarget::Slots(&[0, 2]),
        "start" => EdgeTarget::Start,
        "end" => EdgeTarget::End,
        _ => return None,
    })
}

/// The suffixes of `radius_*`, over corners ordered `[top_left, top_right, bottom_right, bottom_left]` — the order `BorderRadius` declares them in, and the one CSS writes its shorthand in.
///
/// `top`/`start` and their opposites name a *side* here rather than a corner, since that is the shape the property is actually asked for: a panel meeting a rail is rounded on the two corners facing away from it.
pub fn corner_target(suffix: &str) -> Option<EdgeTarget> {
    Some(match suffix {
        "top" => EdgeTarget::Slots(&[0, 1]),
        "bottom" => EdgeTarget::Slots(&[2, 3]),
        "left" => EdgeTarget::Slots(&[0, 3]),
        "right" => EdgeTarget::Slots(&[1, 2]),
        "top_left" => EdgeTarget::Slots(&[0]),
        "top_right" => EdgeTarget::Slots(&[1]),
        "bottom_right" => EdgeTarget::Slots(&[2]),
        "bottom_left" => EdgeTarget::Slots(&[3]),
        "start" => EdgeTarget::Start,
        "end" => EdgeTarget::End,
        _ => return None,
    })
}

/// A property's four edges as the author left them, with the two the writing direction still has to resolve kept apart.
#[derive(Default)]
pub struct Edges {
    /// The value expression for each slot, or `None` where the author named no edge.
    pub slots: [Option<String>; 4],
    pub start: Option<String>,
    pub end: Option<String>,
    /// Set when the property still reduces to its one-value form — a bare shorthand of a single token and no edge named separately. This is what keeps `radius:8` emitting `BorderRadius::all(8.0)` and a plain `stroke_width:1` emitting no per-side data at all.
    pub uniform: Option<String>,
}

impl Edges {
    /// Whether the author named any edge at all, so a property with nothing written can fall back to its default rather than to four explicit zeroes.
    pub fn is_empty(&self) -> bool {
        self.uniform.is_none()
            && self.start.is_none()
            && self.end.is_none()
            && self.slots.iter().all(Option::is_none)
    }

    /// The four slots with `fallback` standing in for every edge the author did not name.
    ///
    /// Zero is the right fallback for both properties, and it is what CSS resolves to as well: a `border-bottom` on its own leaves the other three sides with no style and therefore no width.
    pub fn resolved(&self, fallback: &str) -> [String; 4] {
        std::array::from_fn(|i| {
            self.slots[i]
                .clone()
                .unwrap_or_else(|| fallback.to_string())
        })
    }

    /// `Some(expr)` / `None` rendered as the `Option<f32>` argument the logical helpers take.
    pub fn logical_args(&self) -> (String, String) {
        let arg = |v: &Option<String>| match v {
            Some(e) => format!("Some({e})"),
            None => "None".to_string(),
        };
        (arg(&self.start), arg(&self.end))
    }

    pub fn has_logical(&self) -> bool {
        self.start.is_some() || self.end.is_some()
    }
}

/// Expands a CSS-style shorthand into its four values, with CSS's own arities: one value for all four, two for the two axes, three with the last axis' pair split, four verbatim. `None` for no tokens or more than four, which leaves the value for the caller to reject.
pub fn expand_shorthand(value: &str) -> Option<[&str; 4]> {
    match value.split_whitespace().collect::<Vec<_>>().as_slice() {
        [a] => Some([a, a, a, a]),
        [a, b] => Some([a, b, a, b]),
        [a, b, c] => Some([a, b, c, b]),
        [a, b, c, d] => Some([a, b, c, d]),
        _ => None,
    }
}

/// Reads one property's edges out of an element's attributes: the bare shorthand under `base`, then every `prefix`-suffixed name `target` recognises, applied most-general first.
pub fn collect(
    attrs: &[Attr],
    base: &str,
    prefix: &str,
    target: fn(&str) -> Option<EdgeTarget>,
) -> Edges {
    let mut named: Vec<(EdgeTarget, &str)> = attrs
        .iter()
        .filter_map(|a| {
            let suffix = a.key.strip_prefix(prefix)?;
            Some((target(suffix)?, a.value.text()))
        })
        .collect();
    named.sort_by_key(|(t, _)| t.specificity());

    let bare = attrs
        .iter()
        .find(|a| a.key == base)
        .map(|a| a.value.text().trim());
    let mut edges = Edges::default();

    // The one-value form survives only if it is the whole story: a single token, and no edge named apart from it.
    if let Some(value) = bare
        && named.is_empty()
        && value.split_whitespace().count() == 1
    {
        edges.uniform = Some(crate::style::number_or(value, ZERO));
        return edges;
    }

    if let Some(values) = bare.and_then(expand_shorthand) {
        for (slot, value) in edges.slots.iter_mut().zip(values) {
            *slot = Some(crate::style::number_or(value, ZERO));
        }
    }
    for (t, value) in named {
        let value = crate::style::number_or(value.trim(), ZERO);
        match t {
            EdgeTarget::Slots(indices) => {
                for i in indices {
                    edges.slots[*i] = Some(value.clone());
                }
            }
            EdgeTarget::Start => edges.start = Some(value),
            EdgeTarget::End => edges.end = Some(value),
        }
    }
    edges
}

#[cfg(test)]
#[path = "edges_test.rs"]
mod tests;
