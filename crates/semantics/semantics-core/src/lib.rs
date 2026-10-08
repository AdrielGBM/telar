//! What a thing in an interface *is*, as opposed to where it is or what it looks like.
//!
//! One vocabulary, three answers. A screen reader on the desktop is told a box is the navigation; a document backend makes it a `<nav>`; a terminal writes it into a plain-text reading of the screen. None of the three is the source: the widget said what it was, once, and each target says that in its own idiom.
//!
//! ## Why these words and not HTML's
//!
//! The names here are the ARIA roles, which is what HTML's sectioning elements are a shorthand *for* — `<header>` is `banner`, `<nav>` is `navigation`, `<main>` is `main`. Taking the roles rather than the tags is what keeps this from being a web vocabulary that native has to translate out of: AccessKit models the same set, so the desktop mapping is as exact as the document one, and an application never writes a tag.
//!
//! ## Why it is its own crate
//!
//! `platform-core` and `renderer-core` are siblings — neither may depend on the other — and both need this. It lived in both, differently, which is how a checkbox came to be announced as a checkbox on the desktop and drawn as an anonymous box in a browser.

#![forbid(unsafe_code)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::sync::Arc;

mod destination;
mod keys;
mod location;
mod outline;
mod reading;

pub use destination::{CurrentKind, Destination, Uri};
pub use keys::ConsumedKeys;
pub use location::Location;
pub use outline::SetPosition;
pub use reading::{NumericValue, Orientation};

/// What a box is.
///
/// Deliberately not open-ended. Each variant has to earn itself by changing what at least one target does with it — a role that lands on the same element with the same attributes and the same announcement is a role that does not exist.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Role {
    /// A box that only groups. The overwhelming majority, and the right answer when nothing else fits: saying nothing is better than saying something wrong.
    #[default]
    Group,

    // The regions a screen is made of: what a reader jumps between and a document sections with.
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
    /// Everything under it is geometry in the box's own coordinates. A target that draws pixels ignores the distinction; one that builds a document cannot, because it can draw those but not place them.
    Drawing,
    /// A window within the screen that takes the interaction until it is dismissed.
    Dialog,

    // The things a person operates.
    /// Something pressable, whatever it is drawn as. The right answer for anything whose whole meaning is "activating this does something".
    Button,
    /// A link. Where it goes is [`Semantics::link`], because that is data about the link rather than part of what it is — and keeping it out is what lets a role stay `Copy` and free to compare.
    Link,
    /// Carries a checked state that is part of what it is, not of what it looks like.
    CheckBox,
    /// One of a set where choosing it unchooses the others.
    Radio,
    /// A checkbox that reads as a switch: on or off rather than ticked or not.
    Switch,
    /// The row a set of [`Tab`](Self::Tab)s sits in, and what names the set.
    TabList,
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
    /// A hierarchy of rows that expand and collapse, such as a file or component tree. Its rows are [`TreeItem`](Self::TreeItem)s.
    Tree,
    /// A row of a [`Tree`](Self::Tree): chosen, and expanded or collapsed when it has children.
    TreeItem,
    /// A row of controls that is one Tab stop and is walked with the arrows.
    Toolbar,
    /// The draggable divider between two panes, moved with the arrows as well as by pointer.
    Splitter,
    /// Text that reports the application's state and is announced when it changes without taking focus: "Saved", "3 results".
    Status,
    /// A feed of entries added over time, newest last, announced as they arrive without taking focus: a console, a build output.
    Log,
    /// Not a control at all: text the interface is showing. Never focusable — it is here because a reader given only the buttons cannot say what the buttons are for.
    Label,
}

impl Role {
    /// Whether this is one of the regions a screen is made of, rather than something operated or shown.
    ///
    /// The distinction a document backend needs to decide between a sectioning element and a `<div>`, and the one a reader needs to offer "jump to region".
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
                | Self::TreeItem
                | Self::Splitter
        )
    }

    /// Whether a named box with this role lends its name to the control inside it, as `<label>` does for its field, rather than naming itself: it has no role of its own, holds exactly one control, and draws no words or artwork outside that control. Every target applies this one rule, so a wrapped control is named the same wherever it is read.
    pub fn lends_name_to_control(
        &self,
        drew_beside_controls: bool,
        controls_inside: usize,
    ) -> bool {
        *self == Self::Group && !drew_beside_controls && controls_inside == 1
    }

    /// The name this role goes by in markup and in a stylesheet — the ARIA role, which is also what an application writes in `.rsx`.
    ///
    /// One table rather than one per target: a name that parses and a name that is announced must be the same word, or an author has learned two vocabularies for one idea.
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
            Self::TabList => "tablist",
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
            Self::Tree => "tree",
            Self::TreeItem => "treeitem",
            Self::Toolbar => "toolbar",
            Self::Splitter => "separator",
            Self::Status => "status",
            Self::Log => "log",
            Self::Label => "label",
        }
    }

    /// What [`Semantics::toggled`] means on this role, or `None` for a role that carries no on/off state.
    ///
    /// One flag read through the role, as AccessKit models it, rather than a flag per meaning: a pressed checkbox or a checked tab is a state no target can say, and a single flag cannot be set to one.
    pub fn toggle_kind(&self) -> Option<ToggleKind> {
        match self {
            Self::CheckBox | Self::Radio | Self::Switch => Some(ToggleKind::Checked),
            Self::Button => Some(ToggleKind::Pressed),
            Self::Tab | Self::TreeItem => Some(ToggleKind::Selected),
            Self::Disclosure => Some(ToggleKind::Expanded),
            _ => None,
        }
    }

    /// The role a name spells, for the one place a name arrives as text: what an application wrote.
    ///
    /// Aliases are the words people reach for first. `nav` and `sidebar` are not ARIA — they are what an author types — and pointing them at the role they mean is cheaper than being asked why `sidebar` is spelled `complementary`.
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
            "tablist" => Self::TabList,
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
            "tree" => Self::Tree,
            "treeitem" => Self::TreeItem,
            "toolbar" => Self::Toolbar,
            "splitter" | "separator" => Self::Splitter,
            "status" => Self::Status,
            "log" => Self::Log,
            "label" | "text" => Self::Label,
            _ => return None,
        })
    }
}

/// What a control's on/off state is, as its [`Role::toggle_kind`] decides.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ToggleKind {
    /// A checkbox ticked, a radio chosen, a switch on.
    Checked,
    /// A toggle button held down: a button that stays pressed until pressed again, such as bold in a toolbar.
    Pressed,
    /// The tab whose panel is showing, or the chosen row of a tree.
    Selected,
    /// A disclosure whose content is showing.
    Expanded,
}

/// What a box is, and what should be said about it.
///
/// Carried alongside geometry rather than inside it: two boxes with the same rect can mean entirely different things, and the thing that draws them needs both.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Semantics {
    pub role: Role,
    /// The name assistive technology reads, when the box's own content is not it — an icon-only button.
    ///
    /// Left `None` wherever the drawn text already says it, which is the common case and the one that cannot fall out of step with what is on screen.
    pub label: Option<Arc<str>>,
    /// Where a [`Link`](Role::Link) goes. Absent on every other role.
    pub link: Option<Destination>,
    /// Whether the box is where the keyboard currently is.
    ///
    /// A target that draws its own focus ring does not need telling; one that hands the box to a document does, because the document has a focus of its own and the two must be the same box.
    pub focused: bool,
    /// Whether a control carrying an on/off state is in it; what that state is — checked, pressed, selected, expanded — is its role's [`Role::toggle_kind`]. `None` for the roles that have no such state — never a default of `false` for the ones that do, which announces every checkbox as unticked.
    pub toggled: Option<bool>,
    /// Whether the box is present but not operable. Announced rather than hidden: a control that is genuinely not there is absent instead.
    pub disabled: bool,
    /// Whether the box refuses pointer events, so what is drawn under it takes them instead.
    pub click_through: bool,
    /// How the keyboard reaches the box. `None` for a box that cannot hold focus.
    pub focusable: Option<Focusable>,
    /// The language the box and everything under it is written in, as a BCP 47 tag, where it differs from the one around it. `None` inherits.
    pub lang: Option<Arc<str>>,
    /// Whether assistive technology skips the box and everything under it. Still drawn and still operable by pointer: this is what a reader is told, not what is on screen.
    pub hidden: bool,
    /// The name a [`Destination::Anchor`] reaches this box by: a place on the page.
    pub anchor: Option<Arc<str>>,
    /// Whether a link is the current one of its set: the page being shown, the place on the page the reader is at. Read through [`current_kind`](Self::current_kind), since what it is the current one of is its destination's to say.
    pub current: bool,
    /// Where a control that carries a number stands. `None` for the roles that carry none.
    pub value: Option<NumericValue>,
    /// The axis a splitter, slider or other oriented control runs along. `None` where the role has no axis or none was declared.
    pub orientation: Option<Orientation>,
    /// Whether a row that has rows under it shows them; apart from [`toggled`](Self::toggled) because a tree row is selected and expanded at once. `None` for a leaf and for every role that does not expand this way.
    pub expanded: Option<bool>,
    /// Where an item sits in its hierarchy and among its siblings. `None` where a reader can count that for itself.
    pub position: Option<SetPosition>,
    /// The box under this one, by its element id, that the keyboard's cursor rests on while focus stays here: the row of a tree, the item of a toolbar. `None` where there is no cursor, or its box is not built.
    pub active_descendant: Option<u64>,
}

/// What an application said about a box, laid over what the widget derived.
///
/// Apart from [`Semantics`] because it is authored rather than derived: a widget knows it is a checkbox and whether it is ticked, but only the application knows the word a row of drawn letters spells, which paragraph is in another language, or which box a link to "contact" means.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Annotation {
    pub label: Option<Arc<str>>,
    pub lang: Option<Arc<str>>,
    pub hidden: bool,
    pub anchor: Option<Arc<str>>,
}

impl Annotation {
    pub fn is_empty(&self) -> bool {
        self.label.is_none() && self.lang.is_none() && !self.hidden && self.anchor.is_none()
    }
}

/// How the keyboard reaches a focusable box, and what it keeps once there.
///
/// Carried on the box because a target that shares the keyboard with its host decides from it synchronously, before the app has seen the key: whether the host's own Tab order stops here, and which keys the host must not act on while the box is focused.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Focusable {
    /// Whether Tab stops here right now: registered as a stop, enabled, and reachable.
    pub tab_stop: bool,
    pub consumes: ConsumedKeys,
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

    pub fn with_lang(mut self, lang: impl Into<Arc<str>>) -> Self {
        self.lang = Some(lang.into());
        self
    }

    pub fn hidden_from_readers(mut self) -> Self {
        self.hidden = true;
        self
    }

    pub fn with_anchor(mut self, anchor: impl Into<Arc<str>>) -> Self {
        self.anchor = Some(anchor.into());
        self
    }

    /// What the widget derived, with what the application said on top: its label and language win, and hiding can only be added.
    pub fn annotated(mut self, annotation: &Annotation) -> Self {
        if let Some(label) = &annotation.label {
            self.label = Some(label.clone());
        }
        if let Some(lang) = &annotation.lang {
            self.lang = Some(lang.clone());
        }
        self.hidden |= annotation.hidden;
        if let Some(anchor) = &annotation.anchor {
            self.anchor = Some(anchor.clone());
        }
        self
    }

    /// A link to `destination`. Sets the role too: a box with somewhere to go is a link whatever else it said.
    pub fn linking_to(mut self, destination: Destination) -> Self {
        self.role = Role::Link;
        self.link = Some(destination);
        self
    }

    /// Marks a link as the current one of its set, or not; see [`current`](Self::current).
    pub fn marked_current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    /// What this box is the current one of: `None` unless it is a link with somewhere to go that is marked current.
    pub fn current_kind(&self) -> Option<CurrentKind> {
        self.link
            .as_ref()
            .filter(|_| self.current)
            .map(Destination::current_kind)
    }

    pub fn focusable(mut self, focusable: Focusable) -> Self {
        self.focusable = Some(focusable);
        self
    }

    pub fn click_through(mut self) -> Self {
        self.click_through = true;
        self
    }

    /// The state a control is in, as the thing operating it reports it.
    pub fn in_state(mut self, focused: bool, toggled: Option<bool>, disabled: bool) -> Self {
        self.focused = focused;
        self.toggled = toggled;
        self.disabled = disabled;
        self
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
