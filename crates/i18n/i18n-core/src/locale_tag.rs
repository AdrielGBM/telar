//! Reading a locale tag as far as Telar's own rules need it: its language and its script.

/// Subtags are separated by `-` in BCP 47 and by `_` in the POSIX form many platforms report, and both are read alike.
const SEPARATORS: [char; 2] = ['-', '_'];

/// A locale tag's language subtag and, when it names one, its script subtag, borrowed as the tag spells them.
///
/// `sr-Cyrl-RS`, `sr_Cyrl_RS` and `SR-cyrl` all read as language `sr` with script `Cyrl`, compared ignoring ASCII case. Extended language subtags (`zh-yue-Hant`) are skipped on the way to the script. Anything after the script — region, variants, extensions — is not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LocaleTag<'a> {
    pub(crate) language: &'a str,
    pub(crate) script: Option<&'a str>,
}

impl<'a> LocaleTag<'a> {
    pub(crate) fn parse(tag: &'a str) -> Self {
        let mut subtags = tag.split(SEPARATORS);
        let language = subtags.next().unwrap_or_default();
        let script = subtags
            .find(|subtag| !is_extended_language(subtag))
            .filter(|subtag| is_script(subtag));
        Self { language, script }
    }
}

/// Whether `a` and `b` are the same tag, ignoring ASCII case and which separator each uses.
pub(crate) fn same_tag(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.chars().zip(b.chars()).all(|(a, b)| {
            a.eq_ignore_ascii_case(&b) || (SEPARATORS.contains(&a) && SEPARATORS.contains(&b))
        })
}

/// `tag` with every subtag separated by `-`, the BCP 47 form every locale is kept and compared in.
pub fn canonical_tag(tag: &str) -> String {
    tag.replace(SEPARATORS, "-")
}

fn is_extended_language(subtag: &str) -> bool {
    subtag.len() == 3 && subtag.bytes().all(|b| b.is_ascii_alphabetic())
}

fn is_script(subtag: &str) -> bool {
    subtag.len() == 4 && subtag.bytes().all(|b| b.is_ascii_alphabetic())
}

#[cfg(test)]
#[path = "locale_tag.test.rs"]
mod tests;
