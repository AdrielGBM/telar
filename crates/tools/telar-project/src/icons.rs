//! `[telar.icons]`: where the ids an application gives the `icon` tag of `telar-icons` resolve, whether the CLI bakes them, and which icon licences it accepts.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// When an icon id is resolved.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum IconMode {
    /// At bake time, every one of them: an id the `.rsx` does not write out literally is an error, and the application needs no network and no icon set to run.
    #[default]
    Baked,
    /// At run time, through the source the application installs with `telar-icons`' `runtime` feature. Nothing is baked.
    Runtime,
    /// Literal ids at bake time, and every other id at run time.
    Both,
}

/// What a set that needs a licence decision and is not on the allowlist does to a bake.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum UnlistedLicense {
    #[default]
    Warn,
    Fail,
}

/// `[telar.icons.licenses]`.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IconLicensesSection {
    /// SPDX ids accepted for every set under them, and set prefixes accepted whatever their licence.
    #[serde(default)]
    pub allow: Vec<String>,
    /// What an attribution, copyleft, restricted or unknown licence that `allow` does not name does: `"warn"` (the default) or `"fail"`.
    pub unlisted: Option<UnlistedLicense>,
}

/// `[telar.icons]`.
///
/// The sources are asked in the order own SVGs, Iconify sets, provider, so an application can redraw one icon of a set under the set's own name. None is defaulted: an id resolves only against what the package names here, and no provider is reached unless `provider` names it.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IconsSection {
    pub mode: Option<IconMode>,
    /// A directory of Iconify JSON sets, joined onto the package root: a `node_modules`, the `@iconify/json` package, or a folder of `<set>.json` files.
    pub iconify: Option<String>,
    /// A folder of the application's own SVGs, `<set>/<name>.svg`, joined onto the package root.
    pub svg: Option<String>,
    /// The base URL of an Iconify-compatible API the bake may fetch from, such as a self-hosted instance.
    pub provider: Option<String>,
    #[serde(default)]
    pub licenses: IconLicensesSection,
}

impl IconsSection {
    pub fn mode(&self) -> IconMode {
        self.mode.unwrap_or_default()
    }

    pub fn iconify_dir(&self, package_root: &Path) -> Option<PathBuf> {
        self.iconify.as_deref().map(|dir| package_root.join(dir))
    }

    pub fn svg_dir(&self, package_root: &Path) -> Option<PathBuf> {
        self.svg.as_deref().map(|dir| package_root.join(dir))
    }

    pub fn unlisted_license(&self) -> UnlistedLicense {
        self.licenses.unlisted.unwrap_or_default()
    }

    pub(crate) fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (key, value) in [("iconify", &self.iconify), ("svg", &self.svg)] {
            if value.as_deref().is_some_and(|dir| dir.trim().is_empty()) {
                problems.push(format!(
                    "`[telar.icons] {key} = \"\"` names no directory; remove the key instead"
                ));
            }
        }
        if let Some(provider) = &self.provider
            && !crate::is_absolute_url(provider.trim())
        {
            problems.push(format!(
                "`[telar.icons] provider = \"{provider}\"` is not an http(s) URL, like \"https://icons.example.com\""
            ));
        }
        let names_a_source =
            self.iconify.is_some() || self.svg.is_some() || self.provider.is_some();
        if self.mode() != IconMode::Runtime && !names_a_source {
            problems.push(
                "`[telar.icons]` bakes icons but names nowhere to read them from: set `svg` (a folder of your own SVGs), `iconify` (a directory of Iconify JSON sets) or `provider` (an Iconify-compatible API), or `mode = \"runtime\"` to resolve them as the application runs".to_string(),
            );
        }
        if let Some(entry) = self
            .licenses
            .allow
            .iter()
            .find(|entry| entry.trim().is_empty())
        {
            problems.push(format!(
                "`[telar.icons.licenses] allow` holds an empty entry ({entry:?}); name an SPDX id like \"CC-BY-4.0\" or a set prefix"
            ));
        }
        problems
    }
}

#[cfg(test)]
#[path = "icons_test.rs"]
mod tests;
