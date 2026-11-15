//! The byte spelling of what a box is, for a guest's frame and its reader's snapshot to cross to the host.
//!
//! A link crosses as text and is parsed again on arrival, so the host's own URI policy is the one that decides: a link to a scheme the host refuses arrives as no link at all, and the box that carried it still arrives and still draws. That is [`take_link`]'s whole difference from an ordinary option.

use wire_core::{
    Reader, Shaped, Wire, WireError, Writer, fold, fold_fields, fold_shape, shape_named, wire_enum,
    wire_struct,
};

use crate::{
    ConsumedKeys, CurrentKind, Destination, Focusable, ForcedColors, HasPopup, HeadingLevel, Link,
    LocaleTag, Location, Modal, NumericValue, Orientation, Role, RouteAddress, Semantics,
    SetPosition, SortDirection, Toggle, Uri, UriError, Value,
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
    Link(link) = 17,
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
    KeyboardInput = 48,
    Alert = 49,
    RadioGroup = 50,
});
wire_enum!(HeadingLevel {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
});
wire_enum!(Toggle {
    Off = 0,
    On = 1,
    Mixed = 2
});
wire_enum!(ForcedColors {
    Adjust = 0,
    Keep = 1
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

impl Shaped for NumericValue {
    const SHAPE: u64 = {
        let hash = fold_shape(fold(shape_named("NumericValue"), b"now"), f64::SHAPE);
        let hash = fold_shape(fold(hash, b"min"), f64::SHAPE);
        fold_shape(fold(hash, b"max"), f64::SHAPE)
    };
}

impl Wire for NumericValue {
    fn put(&self, w: &mut Writer) {
        self.now().put(w);
        self.min().put(w);
        self.max().put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let (now, min, max) = (f64::take(r)?, f64::take(r)?, f64::take(r)?);
        NumericValue::new(min..=max, now).map_err(|_| WireError::Invalid("numeric reading"))
    }
}
wire_enum!(Value {
    Number { reading, text } = 0,
    Text(text) = 1,
});
wire_struct!(SetPosition {
    level,
    position,
    size
});
wire_struct!(Modal { layer, opened_from });
wire_struct!(Focusable { tab_stop, consumes });

impl Shaped for Location {
    const SHAPE: u64 = fold_shape(
        fold_shape(
            fold_shape(
                fold_shape(shape_named("Location"), <Vec<String>>::SHAPE),
                <Option<LocaleTag>>::SHAPE,
            ),
            <Option<String>>::SHAPE,
        ),
        <Vec<(String, String)>>::SHAPE,
    );
}

impl Wire for Location {
    fn put(&self, w: &mut Writer) {
        w.len(self.segments().len());
        for segment in self.segments() {
            w.str(segment);
        }
        self.locale().cloned().put(w);
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
        if let Some(locale) = <Option<LocaleTag> as Wire>::take(r)? {
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
impl Shaped for Uri {
    const SHAPE: u64 = fold_shape(shape_named("Uri"), String::SHAPE);
}

impl Wire for Uri {
    fn put(&self, w: &mut Writer) {
        w.str(self.as_str());
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Uri::parse(r.str()?).map_err(|_| WireError::Invalid("a URI this side's policy refuses"))
    }
}

/// Parsed again on arrival, so bytes that are no language tag are refused rather than carried as one.
impl Shaped for LocaleTag {
    const SHAPE: u64 = fold_shape(shape_named("LocaleTag"), String::SHAPE);
}

impl Wire for LocaleTag {
    fn put(&self, w: &mut Writer) {
        w.str(&self.to_string());
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        LocaleTag::parse(r.str()?).map_err(|_| WireError::Invalid("a locale tag"))
    }
}

impl Shaped for RouteAddress {
    const SHAPE: u64 = fold_shape(shape_named("RouteAddress"), String::SHAPE);
}

impl Wire for RouteAddress {
    fn put(&self, w: &mut Writer) {
        w.str(self.as_str());
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        RouteAddress::parse(r.str()?).map_err(|_| WireError::Invalid("a route address"))
    }
}

impl Shaped for ConsumedKeys {
    const SHAPE: u64 = fold_shape(shape_named("ConsumedKeys"), u16::SHAPE);
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

/// What [`put_link`] spells a link as: whether there is one, then its kind's tag and what that kind carries.
pub const LINK_SHAPE: u64 = {
    let hash = fold(shape_named("link"), &[ROUTE]);
    let hash = fold_shape(hash, Location::SHAPE);
    let hash = fold(hash, &[ANCHOR]);
    let hash = fold_shape(hash, <std::sync::Arc<str>>::SHAPE);
    let hash = fold(hash, &[LOCALE]);
    let hash = fold_shape(hash, LocaleTag::SHAPE);
    let hash = fold(hash, &[EXTERNAL]);
    fold_shape(hash, Uri::SHAPE)
};

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

impl Shaped for Link {
    const SHAPE: u64 = fold_fields!(
        fold_shape(fold(shape_named("Link"), b"destination"), LINK_SHAPE),
        Link { current }
    );
}

impl Wire for Link {
    fn put(&self, w: &mut Writer) {
        put_link(w, self.destination.as_ref());
        self.current.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            destination: take_link(r)?,
            current: Wire::take(r)?,
        })
    }
}

wire_struct!(Semantics {
    role,
    label,
    description,
    focused,
    toggled,
    disabled,
    invalid,
    required,
    readonly,
    click_through,
    focusable,
    lang,
    hidden,
    anchor,
    value,
    orientation,
    expanded,
    position,
    active_descendant,
    controls,
    has_popup,
    keyboard_shortcut,
    sort,
    multiselectable,
    modal,
});

#[cfg(test)]
#[path = "wire.test.rs"]
mod tests;
