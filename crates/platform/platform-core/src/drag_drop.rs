//! What a drag carries between surfaces and processes: payloads under named MIME types, the actions a source allows, what a target answers, and how the drag ended.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

/// A `type/subtype` name, optionally followed by `;` parameters, checked when it is made: at compile time for a constant, at run time for a name that arrives from elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MimeType(Cow<'static, str>);

impl MimeType {
    pub const TEXT_PLAIN: Self = Self::from_static("text/plain");
    pub const TEXT_PLAIN_UTF8: Self = Self::from_static("text/plain;charset=utf-8");
    pub const TEXT_MARKDOWN: Self = Self::from_static("text/markdown");
    pub const URI_LIST: Self = Self::from_static("text/uri-list");

    pub const fn from_static(name: &'static str) -> Self {
        assert!(is_well_formed(name), "a MIME type is `type/subtype`");
        Self(Cow::Borrowed(name))
    }

    pub fn parse(name: impl Into<String>) -> Result<Self, MimeTypeError> {
        let name = name.into();
        if is_well_formed(&name) {
            Ok(Self(Cow::Owned(name)))
        } else {
            Err(MimeTypeError(name))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimeTypeError(pub String);

impl fmt::Display for MimeTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not a `type/subtype` MIME type", self.0)
    }
}

impl std::error::Error for MimeTypeError {}

const fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_graphic()
        && !matches!(
            byte,
            b'(' | b')'
                | b'<'
                | b'>'
                | b'@'
                | b','
                | b';'
                | b':'
                | b'\\'
                | b'"'
                | b'/'
                | b'['
                | b']'
                | b'?'
                | b'='
        )
}

const fn is_well_formed(name: &str) -> bool {
    let bytes = name.as_bytes();
    let mut i = 0;
    let mut type_len = 0;
    while i < bytes.len() && bytes[i] != b'/' {
        if !is_token_byte(bytes[i]) {
            return false;
        }
        type_len += 1;
        i += 1;
    }
    if type_len == 0 || i == bytes.len() {
        return false;
    }
    i += 1;
    let mut subtype_len = 0;
    while i < bytes.len() && bytes[i] != b';' {
        if !is_token_byte(bytes[i]) {
            return false;
        }
        subtype_len += 1;
        i += 1;
    }
    if subtype_len == 0 {
        return false;
    }
    while i < bytes.len() {
        if !(bytes[i].is_ascii_graphic() || bytes[i] == b' ') {
            return false;
        }
        i += 1;
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Representation {
    pub mime: MimeType,
    pub bytes: Arc<[u8]>,
}

/// What a drag carries: the same content under each MIME type it offers, the one its source prefers first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DragData {
    pub(crate) representations: Vec<Representation>,
}

impl DragData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `bytes` under `mime`, replacing what that type carried before while keeping its place in the order.
    pub fn with(mut self, mime: MimeType, bytes: impl Into<Arc<[u8]>>) -> Self {
        let bytes = bytes.into();
        match self.representations.iter_mut().find(|r| r.mime == mime) {
            Some(existing) => existing.bytes = bytes,
            None => self.representations.push(Representation { mime, bytes }),
        }
        self
    }

    /// Adds `text` under both plain-text names, since some programs ask only for the one without a charset.
    pub fn with_text(self, text: &str) -> Self {
        let bytes: Arc<[u8]> = Arc::from(text.as_bytes());
        self.with(MimeType::TEXT_PLAIN_UTF8, bytes.clone())
            .with(MimeType::TEXT_PLAIN, bytes)
    }

    pub fn types(&self) -> impl Iterator<Item = &MimeType> {
        self.representations.iter().map(|r| &r.mime)
    }

    pub fn bytes(&self, mime: &MimeType) -> Option<&Arc<[u8]>> {
        self.representations
            .iter()
            .find(|r| &r.mime == mime)
            .map(|r| &r.bytes)
    }

    pub fn representations(&self) -> &[Representation] {
        &self.representations
    }

    pub fn is_empty(&self) -> bool {
        self.representations.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DropAction {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragActions {
    Copy,
    Move,
    CopyOrMove,
}

impl DragActions {
    pub fn allows(self, action: DropAction) -> bool {
        matches!(
            (self, action),
            (Self::CopyOrMove, _) | (Self::Copy, DropAction::Copy) | (Self::Move, DropAction::Move)
        )
    }

    /// `preferred` when allowed, else the one action that is.
    pub fn choose(self, preferred: DropAction) -> DropAction {
        match self {
            Self::CopyOrMove => preferred,
            Self::Copy => DropAction::Copy,
            Self::Move => DropAction::Move,
        }
    }
}

/// What a drag hovering a surface offers it, before anything is transferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragOffer {
    pub types: Vec<MimeType>,
    pub actions: DragActions,
}

impl DragOffer {
    pub fn of(data: &DragData, actions: DragActions) -> Self {
        Self {
            types: data.types().cloned().collect(),
            actions,
        }
    }

    pub fn first_of<'a>(&self, wanted: &'a [MimeType]) -> Option<&'a MimeType> {
        wanted.iter().find(|mime| self.types.contains(mime))
    }
}

/// Which type a surface would take a drop under at the point the drag hovers, and what the drop does with it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DropAnswer {
    pub mime: MimeType,
    pub action: DropAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedData {
    pub mime: MimeType,
    pub bytes: Arc<[u8]>,
    pub action: DropAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragOutcome {
    /// The platform never began it: no drag support here, or the press it needed was already over.
    NotStarted,
    Cancelled,
    Dropped(DropAction),
}

/// The picture that follows the pointer during a drag: premultiplied RGBA pixels drawn at `scale`, held at a hotspot in logical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragIcon {
    pub(crate) rgba: Arc<[u8]>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) scale: u32,
    pub(crate) hotspot: (i32, i32),
}

impl DragIcon {
    /// `None` when the pixels are not `width`×`height` RGBA, or there are none.
    pub fn new(rgba: impl Into<Arc<[u8]>>, width: u32, height: u32) -> Option<Self> {
        let rgba = rgba.into();
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (expected > 0 && rgba.len() == expected).then_some(Self {
            rgba,
            width,
            height,
            scale: 1,
            hotspot: (0, 0),
        })
    }

    pub fn at_scale(mut self, scale: u32) -> Self {
        self.scale = scale.max(1);
        self
    }

    pub fn held_at(mut self, x: i32, y: i32) -> Self {
        self.hotspot = (x, y);
        self
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn scale(&self) -> u32 {
        self.scale
    }

    pub fn hotspot(&self) -> (i32, i32) {
        self.hotspot
    }
}

/// A drag a surface asks the platform to start, from inside the pointer stroke that began it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingDrag {
    pub data: DragData,
    pub actions: DragActions,
    pub icon: Option<DragIcon>,
}

impl OutgoingDrag {
    pub fn new(data: DragData, actions: DragActions) -> Self {
        Self {
            data,
            actions,
            icon: None,
        }
    }

    pub fn with_icon(mut self, icon: DragIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn offer(&self) -> DragOffer {
        DragOffer::of(&self.data, self.actions)
    }

    /// The representation a drop answered with `answer` receives, when the drag carries that type and allows that action.
    pub fn deliver(&self, answer: &DropAnswer) -> Option<DroppedData> {
        let bytes = self.data.bytes(&answer.mime)?;
        self.actions.allows(answer.action).then(|| DroppedData {
            mime: answer.mime.clone(),
            bytes: bytes.clone(),
            action: answer.action,
        })
    }
}

#[cfg(test)]
#[path = "drag_drop.test.rs"]
mod tests;
