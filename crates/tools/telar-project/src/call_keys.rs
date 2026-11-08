//! The keys a component call keeps for itself, so none of them reaches a prop: the transpiler tells a call's own keys from its props by them, and `#[derive(Props)]` refuses a field named after one, which no call could set.

/// The name assistive technology reads for the item a component returns, as for a built-in tag's box.
pub const A11Y_LABEL: &str = "a11y_label";

/// The language of the item a component returns, as for a built-in tag's box.
pub const LANG: &str = "lang";

/// Whether assistive technology skips the item a component returns.
pub const A11Y: &str = "a11y";

/// What the item a component returns is, in words that describe it from outside.
pub const ROLE: &str = "role";

/// The `Children` prop a child of a call is handed through.
pub const SLOT: &str = "slot";

/// How a call once said its language, refused with the spelling that replaced it.
pub const FORMER_LANG: &str = "a11y_lang";

/// The keys a `box`, `row`, `col` or `grid` paints a state of its own with, refused on a call: a component's states are its own.
pub const STATE_STYLE_KEYS: &[&str] = &[
    "hover_style",
    "active_style",
    "disabled_style",
    "focus_style",
];

/// What `key:` on a component call does in place of setting a prop, where the call keeps `key` for itself.
pub fn kept_by_the_call(key: &str) -> Option<&'static str> {
    match key {
        A11Y_LABEL => Some("names the item the component returns for assistive technology"),
        LANG => Some("says the language of the item the component returns"),
        A11Y => Some("says whether assistive technology skips the item the component returns"),
        ROLE => Some("says what the item the component returns is"),
        SLOT => Some("hands a child of the call through the `Children` prop it names"),
        FORMER_LANG => Some("is refused, as the former spelling of `lang:`"),
        key if STATE_STYLE_KEYS.contains(&key) => Some(
            "is refused, since it paints a state of a `box` and a component's states are its own",
        ),
        _ => None,
    }
}

#[cfg(test)]
#[path = "call_keys.test.rs"]
mod tests;
