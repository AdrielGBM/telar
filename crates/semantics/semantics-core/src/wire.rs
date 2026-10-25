//! The byte spelling of what a box is, for a guest's frame and its reader's snapshot to cross to the host.
//!
//! A link crosses as text and is parsed again on arrival, so the host's own URI policy is the one that decides: a link to a scheme the host refuses arrives as no link at all, and the box that carried it still arrives and still draws. That is [`take_link`]'s whole difference from an ordinary option.

use wire_core::{Reader, Wire, WireError, Writer, wire_enum, wire_struct};

use crate::{
    ConsumedKeys, CurrentKind, Destination, Focusable, HasPopup, Location, Modal, NumericValue,
    Orientation, Role, RouteAddress, Semantics, SetPosition, SortDirection, Uri, UriError,
};

wire_enum!(Role {
    Group = 0,
    Banner = 1,
    Navigation = 2,
    Main = 3,
    Complementary = 4,
    ContentInfo = 5,
    Article = 6,
    Section = 7,
    Form = 8,
    Search = 9,
    Heading(level) = 10,
    List = 11,
    ListItem = 12,
    ScrollArea = 13,
    Drawing = 14,
    Dialog = 15,
    Button = 16,
    Link = 17,
    CheckBox = 18,
    Radio = 19,
    Switch = 20,
    TabList = 21,
    Tab = 22,
    TabPanel = 23,
    Menu = 24,
    MenuItem = 25,
    MenuItemCheckBox = 26,
    MenuItemRadio = 27,
    ListBox = 28,
    Option = 29,
    Slider = 30,
    SpinButton = 31,
    TextInput = 32,
    MultilineTextInput = 33,
    ComboBox = 34,
    Disclosure = 35,
    ProgressBar = 36,
    Tree = 37,
    Grid = 38,
    Row = 39,
    GridCell = 40,
    ColumnHeader = 41,
    TreeItem = 42,
    Toolbar = 43,
    Splitter = 44,
    Status = 45,
    Log = 46,
    Label = 47,
});
wire_enum!(Orientation {
    Horizontal = 0,
    Vertical = 1
});
wire_enum!(SortDirection {
    Ascending = 0,
    Descending = 1
});
wire_enum!(HasPopup {
    Menu = 0,
    ListBox = 1
});
wire_enum!(CurrentKind {
    Page = 0,
    Location = 1,
    Item = 2
});

wire_struct!(NumericValue { now, min, max });
wire_struct!(SetPosition {
    level,
    position,
    size
});
wire_struct!(Modal { layer, opened_from });
wire_struct!(Focusable { tab_stop, consumes });

impl Wire for Location {
    fn put(&self, w: &mut Writer) {
        w.len(self.segments().len());
        for segment in self.segments() {
            w.str(segment);
        }
        self.locale().map(str::to_owned).put(w);
        self.fragment().map(str::to_owned).put(w);
        w.len(self.params().len());
        for (key, value) in self.params() {
            w.str(key);
            w.str(value);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        let segments: Vec<&str> = (0..len).map(|_| r.str()).collect::<Result<_, _>>()?;
        let mut location = Location::from_segments(segments);
        if let Some(locale) = <Option<String> as Wire>::take(r)? {
            location = location.with_locale(locale);
        }
        if let Some(fragment) = <Option<String> as Wire>::take(r)? {
            location = location.with_fragment(fragment);
        }
        let len = r.len()?;
        for _ in 0..len {
            location = location.with_param(r.str()?, r.str()?);
        }
        Ok(location)
    }
}

/// Parsed again on arrival, so a URI whose scheme the reading side's policy refuses is refused as bytes that are no URI.
impl Wire for Uri {
    fn put(&self, w: &mut Writer) {
        w.str(self.as_str());
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Uri::parse(r.str()?).map_err(|_| WireError::Invalid("a URI this side's policy refuses"))
    }
}

impl Wire for RouteAddress {
    fn put(&self, w: &mut Writer) {
        w.str(self.as_str());
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        RouteAddress::parse(r.str()?).map_err(|_| WireError::Invalid("a route address"))
    }
}

impl Wire for ConsumedKeys {
    fn put(&self, w: &mut Writer) {
        self.bits().put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        u16::take(r).map(ConsumedKeys::from_bits)
    }
}

const ROUTE: u8 = 0;
const ANCHOR: u8 = 1;
const LOCALE: u8 = 2;
const EXTERNAL: u8 = 3;

/// Spells a box's link, read back by [`take_link`].
pub fn put_link(w: &mut Writer, link: Option<&Destination>) {
    let Some(link) = link else {
        w.u8(0);
        return;
    };
    w.u8(1);
    match link {
        Destination::Route(location) => {
            w.u8(ROUTE);
            location.put(w);
        }
        Destination::Anchor(name) => {
            w.u8(ANCHOR);
            name.put(w);
        }
        Destination::Locale(locale) => {
            w.u8(LOCALE);
            locale.put(w);
        }
        Destination::External(uri) => {
            w.u8(EXTERNAL);
            w.str(uri.as_str());
        }
    }
}

/// Reads a link [`put_link`] spelled, as no link when it goes outside the app to a scheme this side's policy refuses: a guest may allow a scheme its host never did, and the host's word is the one that holds.
pub fn take_link(r: &mut Reader<'_>) -> Result<Option<Destination>, WireError> {
    if !bool::take(r)? {
        return Ok(None);
    }
    Ok(match r.u8()? {
        ROUTE => Some(Destination::Route(Wire::take(r)?)),
        ANCHOR => Some(Destination::Anchor(Wire::take(r)?)),
        LOCALE => Some(Destination::Locale(Wire::take(r)?)),
        EXTERNAL => match Uri::parse(r.str()?) {
            Ok(uri) => Some(Destination::External(uri)),
            Err(UriError::SchemeNotAllowed(_) | UriError::DeniedScheme(_)) => None,
            Err(_) => return Err(WireError::Invalid("a link that is no URI")),
        },
        tag => {
            return Err(WireError::Tag {
                of: "Destination",
                tag,
            });
        }
    })
}

impl Wire for Semantics {
    fn put(&self, w: &mut Writer) {
        self.role.put(w);
        self.label.put(w);
        put_link(w, self.link.as_ref());
        self.focused.put(w);
        self.toggled.put(w);
        self.disabled.put(w);
        self.click_through.put(w);
        self.focusable.put(w);
        self.lang.put(w);
        self.hidden.put(w);
        self.anchor.put(w);
        self.current.put(w);
        self.value.put(w);
        self.orientation.put(w);
        self.expanded.put(w);
        self.position.put(w);
        self.active_descendant.put(w);
        self.controls.put(w);
        self.has_popup.put(w);
        self.keyboard_shortcut.put(w);
        self.sort.put(w);
        self.multiselectable.put(w);
        self.modal.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            role: Wire::take(r)?,
            label: Wire::take(r)?,
            link: take_link(r)?,
            focused: Wire::take(r)?,
            toggled: Wire::take(r)?,
            disabled: Wire::take(r)?,
            click_through: Wire::take(r)?,
            focusable: Wire::take(r)?,
            lang: Wire::take(r)?,
            hidden: Wire::take(r)?,
            anchor: Wire::take(r)?,
            current: Wire::take(r)?,
            value: Wire::take(r)?,
            orientation: Wire::take(r)?,
            expanded: Wire::take(r)?,
            position: Wire::take(r)?,
            active_descendant: Wire::take(r)?,
            controls: Wire::take(r)?,
            has_popup: Wire::take(r)?,
            keyboard_shortcut: Wire::take(r)?,
            sort: Wire::take(r)?,
            multiselectable: Wire::take(r)?,
            modal: Wire::take(r)?,
        })
    }
}

#[cfg(test)]
#[path = "wire_test.rs"]
mod tests;
