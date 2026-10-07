//! What the licences of the icon sets an application draws ask of it: which need a decision, how an allowlist settles them, and the notice that records every set it ships.

use crate::SetInfo;

/// What a licence asks of an application that ships icons under it, read from its SPDX id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseClass {
    /// No conditions: CC0, the Unlicense, MIT-0.
    PublicDomain,
    /// Ship the notice and do what you like: MIT, Apache-2.0, ISC, the BSDs.
    Permissive,
    /// Credit the author where people can see it: CC-BY.
    Attribution,
    /// Terms that reach into what the icons ship in: GPL, LGPL, MPL, OFL, CC-BY-SA.
    Copyleft,
    /// Not for commercial use, or not to be modified: the CC NC and ND variants.
    Restricted,
    /// No licence declared, or one this table does not know.
    Unknown,
}

const PUBLIC_DOMAIN: &[&str] = &["CC0-1.0", "Unlicense", "0BSD", "MIT-0", "CC-PDDC", "WTFPL"];
const PERMISSIVE: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "ISC",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "Zlib",
    "X11",
    "BSL-1.0",
];
const COPYLEFT_FAMILIES: &[&str] = &[
    "GPL-",
    "LGPL-",
    "AGPL-",
    "MPL-",
    "EPL-",
    "OFL-",
    "CC-BY-SA-",
];

impl LicenseClass {
    pub fn of(spdx: Option<&str>) -> Self {
        let Some(spdx) = spdx.map(str::trim).filter(|spdx| !spdx.is_empty()) else {
            return Self::Unknown;
        };
        let named = |list: &[&str]| list.iter().any(|id| id.eq_ignore_ascii_case(spdx));
        let upper = spdx.to_ascii_uppercase();
        if named(PUBLIC_DOMAIN) {
            Self::PublicDomain
        } else if named(PERMISSIVE) {
            Self::Permissive
        } else if upper.starts_with("CC-BY-") && (upper.contains("-NC") || upper.contains("-ND")) {
            Self::Restricted
        } else if COPYLEFT_FAMILIES
            .iter()
            .any(|family| upper.starts_with(&family.to_ascii_uppercase()))
        {
            Self::Copyleft
        } else if upper.starts_with("CC-BY-") {
            Self::Attribution
        } else {
            Self::Unknown
        }
    }

    /// Whether shipping a set under this licence is a decision the application has to make, rather than a notice it has to keep.
    pub fn needs_decision(self) -> bool {
        !matches!(self, Self::PublicDomain | Self::Permissive)
    }

    fn consequence(self) -> &'static str {
        match self {
            Self::PublicDomain | Self::Permissive => "",
            Self::Attribution => {
                "which requires crediting its author where the application shows its notices"
            }
            Self::Copyleft => "a copyleft licence whose terms can extend to what the icons ship in",
            Self::Restricted => "which forbids commercial use or modification",
            Self::Unknown => "which this tool cannot classify, so its terms need reading",
        }
    }
}

/// What a set that needs a decision and is not on the allowlist does to a bake.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OnUnlisted {
    /// Bakes it and says why it needs a look.
    #[default]
    Warn,
    /// Refuses to bake until the allowlist names it.
    Fail,
}

/// An application's answer to the licences of the sets it draws: the licences and sets it has decided to accept, and what happens to one it has not.
///
/// Public-domain and permissive sets are always accepted; their licences ask for the notice, which the bake writes. An attribution, copyleft, restricted or unknown licence needs a decision: naming its SPDX id in `allow` accepts every set under it, and naming a set's prefix accepts that one set whatever its licence says, which is how a set that declares none is accepted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LicensePolicy {
    pub allow: Vec<String>,
    pub unlisted: OnUnlisted,
}

/// What the policy makes of one set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    Warn(String),
    Fail(String),
}

impl LicensePolicy {
    /// Judges the set `prefix`, described by `set`. `own` is the application's own artwork, which is never judged.
    pub fn judge(&self, prefix: &str, set: Option<&SetInfo>, own: bool) -> Verdict {
        if own {
            return Verdict::Accepted;
        }
        let spdx = set
            .and_then(|set| set.license.as_ref())
            .and_then(|license| license.spdx.as_deref());
        let class = LicenseClass::of(spdx);
        let allowed = self.allow.iter().any(|entry| {
            entry == prefix || spdx.is_some_and(|spdx| entry.eq_ignore_ascii_case(spdx))
        });
        if !class.needs_decision() || allowed {
            return Verdict::Accepted;
        }
        let named = set
            .and_then(|set| set.name.as_deref())
            .map(|name| format!(" ({name})"))
            .unwrap_or_default();
        let licence = match spdx {
            Some(spdx) => format!("is licensed {spdx}, {}", class.consequence()),
            None => "declares no licence, so its terms need reading".to_string(),
        };
        let accept = match spdx {
            Some(spdx) => format!("add \"{spdx}\" (every set under it) or \"{prefix}\" (this set)"),
            None => format!("add \"{prefix}\""),
        };
        match self.unlisted {
            OnUnlisted::Warn => Verdict::Warn(format!(
                "icon set `{prefix}`{named} {licence}. Once that is settled, {accept} to `[telar.icons.licenses] allow` to accept it without this warning"
            )),
            OnUnlisted::Fail => Verdict::Fail(format!(
                "icon set `{prefix}`{named} {licence}, and `[telar.icons.licenses] unlisted = \"fail\"` refuses a set the allowlist does not name. To ship it, {accept} to `allow`; otherwise draw that icon from another set"
            )),
        }
    }
}

/// Prefixes of the Iconify sets made of brand logos.
const BRAND_SETS: &[&str] = &[
    "simple-icons",
    "logos",
    "skill-icons",
    "devicon",
    "devicon-plain",
    "cib",
    "fa-brands",
    "fa6-brands",
    "fa7-brands",
    "bxl",
    "brandico",
    "entypo-social",
    "streamline-logos",
    "token-branded",
];

/// Whether `prefix` is a set of brand logos: one Iconify files under a brands or logos category, or one of the well-known brand sets. Its licence covers the drawings, not the marks they depict.
pub fn is_brand_set(prefix: &str, set: Option<&SetInfo>) -> bool {
    let categorised = set
        .and_then(|set| set.category.as_deref())
        .is_some_and(|category| {
            let category = category.to_ascii_lowercase();
            category.contains("brand") || category.contains("logo")
        });
    categorised || BRAND_SETS.contains(&prefix)
}

const TRADEMARK_NOTE: &str = "These icons depict brands that are trademarks of their owners. The licence above covers the drawings, not the right to use the marks: follow each brand's own guidelines.";

/// One set as the notice lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct NoticeSet {
    pub prefix: String,
    pub set: Option<SetInfo>,
    pub own: bool,
    /// The icon names drawn from it, sorted.
    pub icons: Vec<String>,
}

/// The notice for every set `sets` lists, as plain text an application ships beside its own licence.
pub fn notice(package: &str, sets: &[NoticeSet]) -> String {
    let mut out = format!(
        "Icons in {package}\n\nThe icons below were baked into {package} from these sets, each under the licence its own metadata declares.\n"
    );
    for entry in sets {
        let info = entry.set.as_ref();
        let name = info.and_then(|info| info.name.as_deref());
        out.push('\n');
        match name {
            Some(name) => out.push_str(&format!("{} — {name}\n", entry.prefix)),
            None => out.push_str(&format!("{}\n", entry.prefix)),
        }
        if entry.own {
            out.push_str("  The application's own artwork.\n");
        } else {
            out.push_str(&format!("  Licence: {}\n", describe_license(info)));
        }
        if let Some(author) = info.and_then(|info| info.author.as_ref()) {
            let line = match (author.name.as_deref(), author.url.as_deref()) {
                (Some(name), Some(url)) => format!("{name} <{url}>"),
                (Some(name), None) => name.to_string(),
                (None, Some(url)) => url.to_string(),
                (None, None) => String::new(),
            };
            if !line.is_empty() {
                out.push_str(&format!("  Author: {line}\n"));
            }
        }
        if is_brand_set(&entry.prefix, info) {
            out.push_str(&format!("  Trademarks: {TRADEMARK_NOTE}\n"));
        }
        out.push_str(&format!("  Icons: {}\n", entry.icons.join(", ")));
    }
    out
}

fn describe_license(info: Option<&SetInfo>) -> String {
    let Some(license) = info.and_then(|info| info.license.as_ref()) else {
        return "none declared".to_string();
    };
    let mut text = match (license.title.as_deref(), license.spdx.as_deref()) {
        (Some(title), Some(spdx)) if title != spdx => format!("{title} ({spdx})"),
        (Some(title), _) => title.to_string(),
        (None, Some(spdx)) => spdx.to_string(),
        (None, None) => "none declared".to_string(),
    };
    if let Some(url) = license.url.as_deref() {
        text.push_str(&format!(" <{url}>"));
    }
    text
}

#[cfg(test)]
#[path = "license_test.rs"]
mod tests;
