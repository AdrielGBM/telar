//! What a thing in an interface *is*, as opposed to where it is or what it looks like.
//!
//! One vocabulary, three answers. A screen reader on the desktop is told a box is the navigation; a document
//! backend makes it a `<nav>`; a terminal has nothing to do with it yet and ignores it. None of the three is
//! the source: the widget said what it was, once, and each target says that in its own idiom.
//!
//! ## Why these words and not HTML's
//!
//! The names here are the ARIA roles, which is what HTML's sectioning elements are a shorthand *for* —
//! `<header>` is `banner`, `<nav>` is `navigation`, `<main>` is `main`. Taking the roles rather than the tags
//! is what keeps this from being a web vocabulary that native has to translate out of: AccessKit models the
//! same set, so the desktop mapping is as exact as the document one, and an application never writes a tag.
//!
//! ## Why it is its own crate
//!
//! `platform-core` and `renderer-core` are siblings — neither may depend on the other — and both need this.
//! It lived in both, differently, which is how a checkbox came to be announced as a checkbox on the desktop
//! and drawn as an anonymous box in a browser.

#![forbid(unsafe_code)]

use std::sync::Arc;

/// What a box is.
///
/// Deliberately not open-ended. Each variant has to earn itself by changing what at least one target does
/// with it — a role that lands on the same element with the same attributes and the same announcement is a
/// role that does not exist.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Role {
    /// A box that only groups. The overwhelming majority, and the right answer when nothing else fits:
    /// saying nothing is better than saying something wrong.
    #[default]
    Group,

    // --- The regions a screen is made of. What a reader jumps between and a document sections with.
    /// The screen's own banner: a title bar, a masthead, the row that identifies the application.
    Banner,
    /// A set of links or destinations.
    Navigation,
    /// The one region that is what this screen is *for*. At most one per screen.
    Main,
    /// Supporting content beside the main one — a sidebar, a table of contents, a properties panel.
    Complementary,
    /// The closing region: authorship, version, links away.
    ContentInfo,
    /// A self-contained piece that would still make sense lifted out of the screen it is on.
    Article,
    /// A thematic grouping, usually under a heading.
    Section,
    /// Controls that are filled in and submitted together.
    Form,
    /// A form whose purpose is searching.
    Search,
    /// A heading, and how deep. `1` is the screen's own title.
    Heading(u8),
    /// A list of comparable things, and one of them.
    List,
    ListItem,
    /// A region that scrolls its content.
    ScrollArea,
    /// A box whose content is drawn rather than laid out: a bitmap, vector art, an immediate-mode canvas.
    ///
    /// Everything under it is geometry in the box's own coordinates. A target that draws pixels ignores the
    /// distinction; one that builds a document cannot, because it can draw those but not place them.
    Drawing,
    /// A window within the screen that takes the interaction until it is dismissed.
    Dialog,

    // --- The things a person operates.
    /// Something pressable, whatever it is drawn as. The right answer for anything whose whole meaning is
    /// "activating this does something".
    Button,
    /// A link. Where it goes is [`Semantics::link`], because that is data about the link rather than part
    /// of what it is — and keeping it out is what lets a role stay `Copy` and free to compare.
    Link,
    /// Carries a checked state that is part of what it is, not of what it looks like.
    CheckBox,
    /// One of a set where choosing it unchooses the others.
    Radio,
    /// A checkbox that reads as a switch: on or off rather than ticked or not.
    Switch,
    /// Picks one of several panels.
    Tab,
    /// The panel a [`Tab`](Self::Tab) picks.
    TabPanel,
    /// A row of a menu or a bound list.
    MenuItem,
    /// A continuous value dragged along a track.
    Slider,
    /// A discrete value with a step, typed or nudged.
    SpinButton,
    /// A single-line field.
    TextInput,
    /// A multi-line editor.
    MultilineTextInput,
    /// Opens a list of choices and names the current one.
    ComboBox,
    /// A region that reads as one thing and can be collapsed.
    Disclosure,
    /// How far along something is.
    ProgressBar,
    /// Not a control at all: text the interface is showing. Never focusable — it is here because a reader
    /// given only the buttons cannot say what the buttons are for.
    Label,
}

impl Role {
    /// Whether this is one of the regions a screen is made of, rather than something operated or shown.
    ///
    /// The distinction a document backend needs to decide between a sectioning element and a `<div>`, and
    /// the one a reader needs to offer "jump to region".
    pub fn is_region(&self) -> bool {
        matches!(
            self,
            Self::Banner
                | Self::Navigation
                | Self::Main
                | Self::Complementary
                | Self::ContentInfo
                | Self::Article
                | Self::Section
                | Self::Form
                | Self::Search
        )
    }

    /// Whether a person operates this, as opposed to reading it.
    pub fn is_control(&self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Link
                | Self::CheckBox
                | Self::Radio
                | Self::Switch
                | Self::Tab
                | Self::MenuItem
                | Self::Slider
                | Self::SpinButton
                | Self::TextInput
                | Self::MultilineTextInput
                | Self::ComboBox
                | Self::Disclosure
        )
    }

    /// The name this role goes by in markup and in a stylesheet — the ARIA role, which is also what an
    /// application writes in `.rsx`.
    ///
    /// One table rather than one per target: a name that parses and a name that is announced must be the
    /// same word, or an author has learned two vocabularies for one idea.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Banner => "banner",
            Self::Navigation => "navigation",
            Self::Main => "main",
            Self::Complementary => "complementary",
            Self::ContentInfo => "contentinfo",
            Self::Article => "article",
            Self::Section => "section",
            Self::Form => "form",
            Self::Search => "search",
            Self::Heading(_) => "heading",
            Self::List => "list",
            Self::ListItem => "listitem",
            Self::ScrollArea => "scrollarea",
            Self::Drawing => "drawing",
            Self::Dialog => "dialog",
            Self::Button => "button",
            Self::Link => "link",
            Self::CheckBox => "checkbox",
            Self::Radio => "radio",
            Self::Switch => "switch",
            Self::Tab => "tab",
            Self::TabPanel => "tabpanel",
            Self::MenuItem => "menuitem",
            Self::Slider => "slider",
            Self::SpinButton => "spinbutton",
            Self::TextInput => "textbox",
            Self::MultilineTextInput => "textbox",
            Self::ComboBox => "combobox",
            Self::Disclosure => "button",
            Self::ProgressBar => "progressbar",
            Self::Label => "label",
        }
    }

    /// The role a name spells, for the one place a name arrives as text: what an application wrote.
    ///
    /// Aliases are the words people reach for first. `nav` and `sidebar` are not ARIA — they are what an
    /// author types — and pointing them at the role they mean is cheaper than being asked why `sidebar` is
    /// spelled `complementary`.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "group" | "none" => Self::Group,
            "banner" | "header" => Self::Banner,
            "navigation" | "nav" => Self::Navigation,
            "main" | "content" => Self::Main,
            "complementary" | "aside" | "sidebar" => Self::Complementary,
            "contentinfo" | "footer" => Self::ContentInfo,
            "article" => Self::Article,
            "section" => Self::Section,
            "form" => Self::Form,
            "search" => Self::Search,
            "heading" | "h1" => Self::Heading(1),
            "h2" => Self::Heading(2),
            "h3" => Self::Heading(3),
            "h4" => Self::Heading(4),
            "h5" => Self::Heading(5),
            "h6" => Self::Heading(6),
            "list" => Self::List,
            "listitem" | "item" => Self::ListItem,
            "scrollarea" => Self::ScrollArea,
            "drawing" | "img" | "image" => Self::Drawing,
            "dialog" => Self::Dialog,
            "button" => Self::Button,
            "link" => Self::Link,
            "checkbox" => Self::CheckBox,
            "radio" => Self::Radio,
            "switch" | "toggle" => Self::Switch,
            "tab" => Self::Tab,
            "tabpanel" => Self::TabPanel,
            "menuitem" => Self::MenuItem,
            "slider" => Self::Slider,
            "spinbutton" | "stepper" => Self::SpinButton,
            "textbox" | "textinput" => Self::TextInput,
            "multilinetextbox" | "textarea" => Self::MultilineTextInput,
            "combobox" | "select" => Self::ComboBox,
            "disclosure" | "accordion" => Self::Disclosure,
            "progressbar" | "progress" => Self::ProgressBar,
            "label" | "text" => Self::Label,
            _ => return None,
        })
    }
}

/// What a box is, and what should be said about it.
///
/// Carried alongside geometry rather than inside it: two boxes with the same rect can mean entirely
/// different things, and the thing that draws them needs both.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Semantics {
    pub role: Role,
    /// The name assistive technology reads, when the box's own content is not it — an icon-only button.
    ///
    /// Left `None` wherever the drawn text already says it, which is the common case and the one that cannot
    /// fall out of step with what is on screen.
    pub label: Option<Arc<str>>,
    /// Where a [`Link`](Role::Link) goes. Meaningless on every other role, and absent there.
    pub link: Option<Arc<str>>,
    /// Whether the box refuses pointer events, so what is drawn under it takes them instead.
    pub click_through: bool,
}

impl Semantics {
    pub fn group() -> Self {
        Self::default()
    }

    pub fn of(role: Role) -> Self {
        Self {
            role,
            ..Self::default()
        }
    }

    pub fn drawing() -> Self {
        Self::of(Role::Drawing)
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    pub fn with_label(mut self, label: impl Into<Arc<str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// A link to `target`. Sets the role too: a box with somewhere to go is a link whatever else it said.
    pub fn linking_to(mut self, target: impl Into<Arc<str>>) -> Self {
        self.role = Role::Link;
        self.link = Some(target.into());
        self
    }

    pub fn click_through(mut self) -> Self {
        self.click_through = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_reads_back_as_the_name_it_was_written_with() {
        for name in [
            "banner",
            "navigation",
            "main",
            "complementary",
            "contentinfo",
            "article",
            "section",
            "form",
            "search",
            "list",
            "listitem",
            "button",
            "checkbox",
            "radio",
            "switch",
            "tab",
            "tabpanel",
            "menuitem",
            "slider",
            "spinbutton",
            "combobox",
            "progressbar",
            "label",
            "group",
        ] {
            let role = Role::parse(name).unwrap_or_else(|| panic!("`{name}` should parse"));
            assert_eq!(role.as_str(), name, "`{name}` should round-trip");
        }
    }

    #[test]
    fn the_words_an_author_reaches_for_first_point_at_the_role_they_mean() {
        assert_eq!(Role::parse("nav"), Some(Role::Navigation));
        assert_eq!(Role::parse("sidebar"), Some(Role::Complementary));
        assert_eq!(Role::parse("header"), Some(Role::Banner));
        assert_eq!(Role::parse("footer"), Some(Role::ContentInfo));
        assert_eq!(Role::parse("h2"), Some(Role::Heading(2)));
    }

    #[test]
    fn a_name_nothing_answers_to_is_not_guessed_at() {
        assert_eq!(Role::parse("aricle"), None);
        assert_eq!(Role::parse(""), None);
    }

    #[test]
    fn a_region_is_not_a_control_and_neither_is_a_plain_box() {
        assert!(Role::Navigation.is_region());
        assert!(!Role::Navigation.is_control());
        assert!(Role::Slider.is_control());
        assert!(!Role::Slider.is_region());
        assert!(!Role::Group.is_region());
        assert!(!Role::Group.is_control());
    }

    #[test]
    fn a_box_is_a_group_until_it_says_otherwise() {
        assert_eq!(Semantics::default().role, Role::Group);
        assert_eq!(Semantics::of(Role::Main).role, Role::Main);
        assert!(Semantics::group().label.is_none());
    }
}
