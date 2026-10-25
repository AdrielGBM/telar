//! The byte spelling of what a host and a guest tell each other besides a frame: input, preferences, window requests, the field taking text and what a reader is told.

use std::ffi::{c_ulong, c_void};
use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;

use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, DrmDisplayHandle, DrmWindowHandle, GbmDisplayHandle,
    GbmWindowHandle, RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
    Win32WindowHandle, WinRtWindowHandle, WindowsDisplayHandle, XcbDisplayHandle, XcbWindowHandle,
    XlibDisplayHandle, XlibWindowHandle,
};
use wire_core::{Reader, Wire, WireError, Writer, wire_enum, wire_struct};

use crate::guest::RawHandles;
use crate::{
    AccessAudit, AccessNode, AccessRule, AccessSeverity, AccessViolation, ColorScheme, Cursor,
    Event, FullscreenMode, HistoryStep, HistoryUpdate, Ime, Key, KeyboardConvention,
    LocationFormat, ModifiersState, NamedKey, PointerButton, PointerSource, ResizeEdge,
    ScrollDelta, SystemPreferences, TextInput, WindowCommand, WindowConfig, WindowPosition,
};

wire_enum!(NamedKey {
    Enter = 0,
    Backspace = 1,
    Escape = 2,
    Tab = 3,
    Delete = 4,
    Home = 5,
    End = 6,
    PageUp = 7,
    PageDown = 8,
    ArrowUp = 9,
    ArrowDown = 10,
    ArrowLeft = 11,
    ArrowRight = 12,
    F1 = 13,
    F2 = 14,
    F3 = 15,
    F4 = 16,
    F5 = 17,
    F6 = 18,
    F7 = 19,
    F8 = 20,
    F9 = 21,
    F10 = 22,
    F11 = 23,
    F12 = 24,
    F13 = 25,
    F14 = 26,
    F15 = 27,
    F16 = 28,
    F17 = 29,
    F18 = 30,
    F19 = 31,
    F20 = 32,
    F21 = 33,
    F22 = 34,
    F23 = 35,
    F24 = 36,
    Space = 37,
    Insert = 38,
    CapsLock = 39,
    ContextMenu = 40,
    Numpad0 = 41,
    Numpad1 = 42,
    Numpad2 = 43,
    Numpad3 = 44,
    Numpad4 = 45,
    Numpad5 = 46,
    Numpad6 = 47,
    Numpad7 = 48,
    Numpad8 = 49,
    Numpad9 = 50,
    NumpadAdd = 51,
    NumpadSubtract = 52,
    NumpadMultiply = 53,
    NumpadDivide = 54,
    NumpadDecimal = 55,
    NumpadEnter = 56,
});
wire_enum!(Key {
    Char(character) = 0,
    Named(key) = 1
});
wire_struct!(ModifiersState {
    is_shift,
    is_ctrl,
    is_alt,
    is_meta
});
wire_enum!(ScrollDelta {
    Lines { x, y } = 0,
    Pixels { x, y } = 1
});
wire_enum!(PointerSource {
    Mouse = 0,
    Touch { id } = 1
});
wire_enum!(PointerButton {
    Primary = 0,
    Secondary = 1,
    Auxiliary = 2
});
wire_enum!(Ime {
    Enabled = 0,
    Preedit { text, cursor } = 1,
    Commit(text) = 2,
    Disabled = 3,
});
wire_enum!(ColorScheme {
    Light = 0,
    Dark = 1
});
wire_struct!(SystemPreferences {
    color_scheme,
    reduced_motion,
    high_contrast,
    locales
});
wire_enum!(Event {
    WindowResized { width, height } = 0,
    WindowCloseRequested = 1,
    FocusChanged { is_focused } = 2,
    CursorEntered = 3,
    CursorLeft = 4,
    ScaleFactorChanged { scale_factor } = 5,
    KeyPressed {
        key,
        modifiers,
        unmodified
    } = 6,
    KeyReleased { key, modifiers } = 7,
    Ime(ime) = 8,
    ModifiersChanged { modifiers } = 9,
    PointerMoved { x, y, source } = 10,
    PointerPressed {
        x,
        y,
        button,
        source
    } = 11,
    PointerReleased {
        x,
        y,
        button,
        source
    } = 12,
    Scrolled { delta, x, y } = 13,
    BoxScrolled { box_id, x, y } = 14,
    ScrollEnded { x, y } = 15,
    BoxFocused { box_id } = 16,
    FocusLeftBoxes = 17,
    BoxActivated { box_id } = 18,
    RunActivated { box_id, run } = 19,
    InSurface { surface, event } = 20,
    AccessAction { id, activate } = 21,
    SafeAreaChanged { insets } = 22,
    SystemPreferencesChanged { preferences } = 23,
    LocationChanged { history } = 24,
});

wire_enum!(Cursor {
    Default = 0,
    Pointer = 1,
    Crosshair = 2,
    Grab = 3,
    Grabbing = 4,
    ColResize = 5,
    RowResize = 6,
    EwResize = 7,
    NsResize = 8,
    NwseResize = 9,
    NeswResize = 10,
    Move = 11,
    Text = 12,
    NotAllowed = 13,
    Wait = 14,
});
wire_enum!(ResizeEdge {
    North = 0,
    South = 1,
    East = 2,
    West = 3,
    NorthEast = 4,
    NorthWest = 5,
    SouthEast = 6,
    SouthWest = 7,
});
wire_enum!(HistoryStep {
    Push(count) = 0,
    Replace = 1,
    Back(count) = 2
});
wire_struct!(HistoryUpdate { step, history });
wire_enum!(WindowCommand {
    Drag = 0,
    DragResize(edge) = 1,
    SetDecorations(on) = 2,
    Minimize = 3,
    ToggleMaximize = 4,
    SetMaximized(on) = 5,
    Close = 6,
    SetTitle(title) = 7,
    Focus = 8,
    SetCursor(cursor) = 9,
    Navigate(update) = 10,
});

wire_enum!(FullscreenMode {
    Disabled = 0,
    Borderless = 1,
    Exclusive = 2
});
wire_enum!(WindowPosition {
    Centered = 0,
    At(x, y) = 1
});
wire_struct!(WindowConfig {
    title,
    width,
    height,
    min_size,
    max_size,
    is_resizable,
    has_decorations,
    is_transparent,
    fullscreen,
    position,
    is_always_on_top,
});

wire_struct!(TextInput {
    field,
    caret,
    secret
});
wire_enum!(KeyboardConvention {
    Mac = 0,
    Pc = 1
});

wire_enum!(AccessSeverity {
    Warning = 0,
    Error = 1
});
wire_enum!(AccessRule {
    UnnamedControl = 0,
    SymbolOnlyName = 1,
    UnnamedPicture = 2,
    Contrast = 3,
    TargetSize = 4,
    HiddenFocusable = 5,
    FocusOrder = 6,
    MissingLang = 7,
});
wire_struct!(AccessViolation {
    rule,
    severity,
    message,
    rect
});
wire_struct!(AccessNode {
    id,
    parent,
    role,
    name,
    rect,
    focused,
    enabled,
    toggled,
    value,
    lang,
    url,
    current,
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
wire_struct!(AccessAudit { nodes, violations });

impl Wire for LocationFormat {
    fn put(&self, w: &mut Writer) {
        w.str(self.base());
        self.trailing_slash().put(w);
        w.len(self.locales().len());
        for locale in self.locales() {
            w.str(locale);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let base = r.str()?;
        Ok(LocationFormat::new(base)
            .with_trailing_slash(bool::take(r)?)
            .with_locales(Vec::<String>::take(r)?))
    }
}

// Only the handles of the desktops a host loads guests on are spelled; a mobile, web, Orbital or Haiku handle crosses as none.
impl Wire for RawHandles {
    fn put(&self, w: &mut Writer) {
        put_window(w, self.window);
        put_display(w, self.display);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            window: take_window(r)?,
            display: take_display(r)?,
        })
    }
}

fn put_window(w: &mut Writer, handle: Option<RawWindowHandle>) {
    match handle {
        Some(RawWindowHandle::AppKit(handle)) => {
            w.u8(1);
            put_ptr(w, Some(handle.ns_view));
        }
        Some(RawWindowHandle::Xlib(handle)) => {
            w.u8(2);
            w.u64(put_ulong(handle.window));
            w.u64(put_ulong(handle.visual_id));
        }
        Some(RawWindowHandle::Xcb(handle)) => {
            w.u8(3);
            handle.window.get().put(w);
            handle.visual_id.map_or(0, NonZeroU32::get).put(w);
        }
        Some(RawWindowHandle::Wayland(handle)) => {
            w.u8(4);
            put_ptr(w, Some(handle.surface));
        }
        Some(RawWindowHandle::Drm(handle)) => {
            w.u8(5);
            handle.plane.put(w);
        }
        Some(RawWindowHandle::Gbm(handle)) => {
            w.u8(6);
            put_ptr(w, Some(handle.gbm_surface));
        }
        Some(RawWindowHandle::Win32(handle)) => {
            w.u8(7);
            w.u64(handle.hwnd.get() as u64);
            w.u64(handle.hinstance.map_or(0, NonZeroIsize::get) as u64);
        }
        Some(RawWindowHandle::WinRt(handle)) => {
            w.u8(8);
            put_ptr(w, Some(handle.core_window));
        }
        _ => w.u8(0),
    }
}

fn take_window(r: &mut Reader<'_>) -> Result<Option<RawWindowHandle>, WireError> {
    Ok(Some(match r.u8()? {
        0 => return Ok(None),
        1 => RawWindowHandle::AppKit(AppKitWindowHandle::new(take_non_null(r)?)),
        2 => {
            let mut handle = XlibWindowHandle::new(take_ulong(r)?);
            handle.visual_id = take_ulong(r)?;
            RawWindowHandle::Xlib(handle)
        }
        3 => {
            let window = NonZeroU32::new(u32::take(r)?)
                .ok_or(WireError::Invalid("an xcb window of id 0"))?;
            let mut handle = XcbWindowHandle::new(window);
            handle.visual_id = NonZeroU32::new(u32::take(r)?);
            RawWindowHandle::Xcb(handle)
        }
        4 => RawWindowHandle::Wayland(WaylandWindowHandle::new(take_non_null(r)?)),
        5 => RawWindowHandle::Drm(DrmWindowHandle::new(u32::take(r)?)),
        6 => RawWindowHandle::Gbm(GbmWindowHandle::new(take_non_null(r)?)),
        7 => {
            let hwnd =
                NonZeroIsize::new(r.u64()? as isize).ok_or(WireError::Invalid("a null hwnd"))?;
            let mut handle = Win32WindowHandle::new(hwnd);
            handle.hinstance = NonZeroIsize::new(r.u64()? as isize);
            RawWindowHandle::Win32(handle)
        }
        8 => RawWindowHandle::WinRt(WinRtWindowHandle::new(take_non_null(r)?)),
        tag => {
            return Err(WireError::Tag {
                of: "RawWindowHandle",
                tag,
            });
        }
    }))
}

fn put_display(w: &mut Writer, handle: Option<RawDisplayHandle>) {
    match handle {
        Some(RawDisplayHandle::AppKit(_)) => w.u8(1),
        Some(RawDisplayHandle::Xlib(handle)) => {
            w.u8(2);
            put_ptr(w, handle.display);
            w.i32(handle.screen);
        }
        Some(RawDisplayHandle::Xcb(handle)) => {
            w.u8(3);
            put_ptr(w, handle.connection);
            w.i32(handle.screen);
        }
        Some(RawDisplayHandle::Wayland(handle)) => {
            w.u8(4);
            put_ptr(w, Some(handle.display));
        }
        Some(RawDisplayHandle::Drm(handle)) => {
            w.u8(5);
            w.i32(handle.fd);
        }
        Some(RawDisplayHandle::Gbm(handle)) => {
            w.u8(6);
            put_ptr(w, Some(handle.gbm_device));
        }
        Some(RawDisplayHandle::Windows(_)) => w.u8(7),
        _ => w.u8(0),
    }
}

fn take_display(r: &mut Reader<'_>) -> Result<Option<RawDisplayHandle>, WireError> {
    Ok(Some(match r.u8()? {
        0 => return Ok(None),
        1 => RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
        2 => RawDisplayHandle::Xlib(XlibDisplayHandle::new(take_ptr(r)?, r.i32()?)),
        3 => RawDisplayHandle::Xcb(XcbDisplayHandle::new(take_ptr(r)?, r.i32()?)),
        4 => RawDisplayHandle::Wayland(WaylandDisplayHandle::new(take_non_null(r)?)),
        5 => RawDisplayHandle::Drm(DrmDisplayHandle::new(r.i32()?)),
        6 => RawDisplayHandle::Gbm(GbmDisplayHandle::new(take_non_null(r)?)),
        7 => RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
        tag => {
            return Err(WireError::Tag {
                of: "RawDisplayHandle",
                tag,
            });
        }
    }))
}

fn put_ptr(w: &mut Writer, ptr: Option<NonNull<c_void>>) {
    w.u64(ptr.map_or(0, |ptr| ptr.as_ptr().expose_provenance() as u64));
}

fn take_ptr(r: &mut Reader<'_>) -> Result<Option<NonNull<c_void>>, WireError> {
    let address = usize::try_from(r.u64()?)
        .map_err(|_| WireError::Invalid("an address wider than this target's"))?;
    Ok(NonNull::new(std::ptr::with_exposed_provenance_mut(address)))
}

fn take_non_null(r: &mut Reader<'_>) -> Result<NonNull<c_void>, WireError> {
    take_ptr(r)?.ok_or(WireError::Invalid("a null window handle"))
}

// `c_ulong` is 32 bits on Windows and 64 elsewhere, so the conversion is only ever useless on one of them.
#[allow(clippy::useless_conversion)]
fn put_ulong(id: c_ulong) -> u64 {
    id.into()
}

#[allow(clippy::useless_conversion, clippy::unnecessary_fallible_conversions)]
fn take_ulong(r: &mut Reader<'_>) -> Result<c_ulong, WireError> {
    c_ulong::try_from(r.u64()?).map_err(|_| WireError::Invalid("an id wider than a C long"))
}

#[cfg(test)]
#[path = "wire_test.rs"]
mod tests;
