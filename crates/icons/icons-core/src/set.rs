//! The Iconify JSON format: one icon set, as `@iconify/json`, an `@iconify-json/<set>` package and an Iconify API all write it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How long an alias chain may be before it is taken for a cycle. Iconify's own tools stop at a depth well below this; a real set nests one or two.
const MAX_ALIAS_DEPTH: usize = 16;

/// What Iconify assumes for a side a set and its icons leave out.
const DEFAULT_SIDE: f32 = 16.0;

/// One Iconify icon set: its icons, the aliases that rename or transform them, the box every icon is drawn in unless it says otherwise, and what the set says about itself.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconSet {
    pub prefix: String,
    #[serde(default)]
    pub icons: BTreeMap<String, IconData>,
    #[serde(default)]
    pub aliases: BTreeMap<String, AliasData>,
    /// The names an Iconify API was asked for and does not have.
    #[serde(default, rename = "not_found")]
    pub not_found: Vec<String>,
    #[serde(flatten)]
    pub defaults: Dimensions,
    #[serde(default)]
    pub info: Option<SetInfo>,
}

/// An icon's box: where its view box starts and how large it is. Each side is optional at every level, and a nearer level wins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct Dimensions {
    pub left: Option<f32>,
    pub top: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Dimensions {
    /// These sides, with each one left out taken from `fallback`.
    fn or(self, fallback: Self) -> Self {
        Self {
            left: self.left.or(fallback.left),
            top: self.top.or(fallback.top),
            width: self.width.or(fallback.width),
            height: self.height.or(fallback.height),
        }
    }
}

/// A rotation in quarter turns and the two mirrorings. Unlike a box, they compose along an alias chain: rotations add and flips toggle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub rotate: Option<i32>,
    pub h_flip: Option<bool>,
    pub v_flip: Option<bool>,
}

/// One drawn icon: its SVG body, the markup inside `<svg>`.
#[derive(Debug, Clone, Deserialize)]
pub struct IconData {
    pub body: String,
    #[serde(flatten)]
    pub dimensions: Dimensions,
    #[serde(flatten)]
    pub transform: Transform,
}

/// Another name for an icon, optionally drawn in a different box or rotated or mirrored.
#[derive(Debug, Clone, Deserialize)]
pub struct AliasData {
    pub parent: String,
    #[serde(flatten)]
    pub dimensions: Dimensions,
    #[serde(flatten)]
    pub transform: Transform,
}

/// What a set says about itself: its name, its author, its licence, and whether its icons carry their own colours.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// `true` for a set whose icons are drawn in fixed colours rather than `currentColor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A set's licence as Iconify records it: a title for people, an SPDX id for tools, and where the text is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct License {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spdx: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// One icon with its aliases followed and the set's defaults applied: everything needed to write it as a standalone SVG document.
#[derive(Debug, Clone, PartialEq)]
pub struct Icon {
    pub body: String,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    /// Quarter turns clockwise, `0..4`.
    pub rotate: i32,
    pub h_flip: bool,
    pub v_flip: bool,
}

impl IconSet {
    /// Parses one set as Iconify writes it.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// The icon named `name`, an alias followed to the icon it names, or `None` when the set has neither or an alias leads nowhere.
    ///
    /// Each alias may set its own box, which wins over its parent's, and its own rotation and mirroring, which compose with its parent's; the set's defaults fill whatever no level set, and Iconify's 16×16 whatever the set left out too.
    pub fn icon(&self, name: &str) -> Option<Icon> {
        let mut chain = Chain::default();
        let mut current = name;
        for _ in 0..MAX_ALIAS_DEPTH {
            if let Some(icon) = self.icons.get(current) {
                chain.compose(icon.dimensions, icon.transform);
                let resolved = chain.dimensions.or(self.defaults);
                return Some(Icon {
                    body: icon.body.clone(),
                    left: resolved.left.unwrap_or(0.0),
                    top: resolved.top.unwrap_or(0.0),
                    width: resolved.width.unwrap_or(DEFAULT_SIDE),
                    height: resolved.height.unwrap_or(DEFAULT_SIDE),
                    rotate: chain.rotate.rem_euclid(4),
                    h_flip: chain.h_flip,
                    v_flip: chain.v_flip,
                });
            }
            let alias = self.aliases.get(current)?;
            chain.compose(alias.dimensions, alias.transform);
            current = &alias.parent;
        }
        None
    }
}

/// What an alias chain has said so far, nearest level first.
#[derive(Default)]
struct Chain {
    dimensions: Dimensions,
    rotate: i32,
    h_flip: bool,
    v_flip: bool,
}

impl Chain {
    fn compose(&mut self, own: Dimensions, transform: Transform) {
        self.dimensions = self.dimensions.or(own);
        self.rotate += transform.rotate.unwrap_or(0);
        self.h_flip ^= transform.h_flip.unwrap_or(false);
        self.v_flip ^= transform.v_flip.unwrap_or(false);
    }
}

impl Icon {
    /// The icon as a standalone SVG document, sized to its view box, with its rotation and mirroring written as a transform around its body the way Iconify's own renderer writes them.
    pub fn to_svg(&self) -> String {
        let (mut left, mut top, mut width, mut height) =
            (self.left, self.top, self.width, self.height);
        let mut rotate = self.rotate;
        let mut transforms: Vec<String> = Vec::new();
        match (self.h_flip, self.v_flip) {
            (true, true) => rotate += 2,
            (true, false) => {
                transforms.push(format!("translate({} {})", num(width + left), num(-top)));
                transforms.push("scale(-1 1)".to_string());
                (left, top) = (0.0, 0.0);
            }
            (false, true) => {
                transforms.push(format!("translate({} {})", num(-left), num(height + top)));
                transforms.push("scale(1 -1)".to_string());
                (left, top) = (0.0, 0.0);
            }
            (false, false) => {}
        }
        let rotate = rotate.rem_euclid(4);
        match rotate {
            1 => {
                let centre = height / 2.0 + top;
                transforms.insert(0, format!("rotate(90 {} {})", num(centre), num(centre)));
            }
            2 => transforms.insert(
                0,
                format!(
                    "rotate(180 {} {})",
                    num(width / 2.0 + left),
                    num(height / 2.0 + top)
                ),
            ),
            3 => {
                let centre = width / 2.0 + left;
                transforms.insert(0, format!("rotate(-90 {} {})", num(centre), num(centre)));
            }
            _ => {}
        }
        if rotate % 2 == 1 {
            (left, top) = (top, left);
            (width, height) = (height, width);
        }
        let body = if transforms.is_empty() {
            self.body.clone()
        } else {
            format!(
                "<g transform=\"{}\">{}</g>",
                transforms.join(" "),
                self.body
            )
        };
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"{} {} {w} {h}\">{body}</svg>",
            num(left),
            num(top),
            w = num(width),
            h = num(height),
        )
    }
}

/// A number as SVG wants it: no trailing `.0` on a whole one.
fn num(value: f32) -> String {
    if value.fract() == 0.0 && value.abs() < 1e9 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
#[path = "set_test.rs"]
mod tests;
