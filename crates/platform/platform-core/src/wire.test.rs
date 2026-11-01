use std::collections::HashSet;
use std::fmt::Debug;

use geometry_core::{Insets, Rect};
use wire_core::{Wire, WireError, decode, encode};

use crate::{
    AccessAudit, AccessNode, AccessRule, AccessSeverity, AccessViolation, ColorScheme, CurrentKind,
    Cursor, Event, FullscreenMode, HasPopup, HistoryStep, HistoryUpdate, Ime, Key,
    KeyboardConvention, Location, ModifiersState, NamedKey, NumericValue, Orientation,
    PointerButton, PointerSource, ResizeEdge, Role, ScrollDelta, SetPosition, SortDirection,
    SystemPreferences, TextInput, WindowCommand, WindowConfig, WindowPosition,
};

fn round_trip<T: Wire + PartialEq + Debug>(value: T) {
    assert_eq!(decode::<T>(&encode(&value)), Ok(value));
}

fn every_tag_decodes_to_a_value_of_its_own<T: Wire + PartialEq + Debug + Clone>(
    count: u8,
    of: &'static str,
) -> Vec<T> {
    let values: Vec<T> = (0..count).map(|tag| decode::<T>(&[tag]).unwrap()).collect();
    for (index, value) in values.iter().enumerate() {
        round_trip(value.clone());
        assert!(!values[..index].contains(value), "{value:?} has two tags");
    }
    assert_eq!(
        decode::<T>(&[count]),
        Err(WireError::Tag { of, tag: count })
    );
    values
}

fn modifiers() -> ModifiersState {
    ModifiersState {
        is_shift: true,
        is_ctrl: false,
        is_alt: true,
        is_meta: false,
    }
}

fn preferences() -> SystemPreferences {
    SystemPreferences {
        color_scheme: Some(ColorScheme::Dark),
        reduced_motion: Some(true),
        high_contrast: None,
        locales: vec!["es-CL".into(), "en".into()],
    }
}

fn history() -> Vec<Location> {
    vec![
        Location::root(),
        Location::from_segments(["docs"]).with_fragment("intro"),
    ]
}

fn every_event() -> Vec<Event> {
    vec![
        Event::WindowResized {
            width: 800,
            height: 600,
        },
        Event::WindowCloseRequested,
        Event::FocusChanged { is_focused: true },
        Event::CursorEntered,
        Event::CursorLeft,
        Event::ScaleFactorChanged { scale_factor: 1.25 },
        Event::KeyPressed {
            key: Key::Char('†'),
            modifiers: modifiers(),
            unmodified: Some(Key::Char('t')),
        },
        Event::KeyPressed {
            key: Key::Named(NamedKey::NumpadEnter),
            modifiers: ModifiersState::default(),
            unmodified: None,
        },
        Event::KeyReleased {
            key: Key::Named(NamedKey::Escape),
            modifiers: modifiers(),
        },
        Event::Ime(Ime::Enabled),
        Event::Ime(Ime::Preedit {
            text: "かな".into(),
            cursor: Some(0..3),
        }),
        Event::Ime(Ime::Preedit {
            text: String::new(),
            cursor: None,
        }),
        Event::Ime(Ime::Commit("仮名".into())),
        Event::Ime(Ime::Disabled),
        Event::ModifiersChanged {
            modifiers: modifiers(),
        },
        Event::PointerMoved {
            x: 1.5,
            y: -2.5,
            source: PointerSource::Mouse,
        },
        Event::PointerPressed {
            x: 3.0,
            y: 4.0,
            button: PointerButton::Primary,
            source: PointerSource::Touch { id: 7 },
        },
        Event::PointerReleased {
            x: 3.0,
            y: 4.0,
            button: PointerButton::Auxiliary,
            source: PointerSource::Mouse,
        },
        Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
            x: 10.0,
            y: 20.0,
        },
        Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 4.5, y: 0.0 },
            x: 0.0,
            y: 0.0,
        },
        Event::BoxScrolled {
            box_id: 42,
            x: 0.0,
            y: 120.0,
        },
        Event::ScrollEnded { x: 5.0, y: 6.0 },
        Event::BoxFocused { box_id: 3 },
        Event::FocusLeftBoxes,
        Event::BoxActivated { box_id: 4 },
        Event::RunActivated { box_id: 5, run: 2 },
        Event::InSurface {
            surface: 9,
            event: Box::new(Event::InSurface {
                surface: 10,
                event: Box::new(Event::AccessAction {
                    id: 11,
                    activate: true,
                }),
            }),
        },
        Event::AccessAction {
            id: 12,
            activate: false,
        },
        Event::SafeAreaChanged {
            insets: Insets::new(44.0, 0.0, 34.0, 0.0),
        },
        Event::SystemPreferencesChanged {
            preferences: preferences(),
        },
        Event::LocationChanged { history: history() },
    ]
}

#[test]
fn every_event_round_trips() {
    let events = every_event();
    let mut tags = HashSet::new();
    for event in events {
        tags.insert(encode(&event)[0]);
        round_trip(event);
    }
    assert_eq!(tags.len(), 25);
    assert_eq!(
        decode::<Event>(&[25]),
        Err(WireError::Tag {
            of: "Event",
            tag: 25
        })
    );
}

#[test]
fn every_named_key_has_a_tag_of_its_own() {
    every_tag_decodes_to_a_value_of_its_own::<NamedKey>(57, "NamedKey");
}

#[test]
fn every_pointer_button_and_cursor_has_a_tag_of_its_own() {
    every_tag_decodes_to_a_value_of_its_own::<PointerButton>(3, "PointerButton");
    every_tag_decodes_to_a_value_of_its_own::<Cursor>(15, "Cursor");
}

#[test]
fn system_preferences_round_trip_known_and_unknown() {
    round_trip(preferences());
    round_trip(SystemPreferences::default());
    round_trip(SystemPreferences {
        color_scheme: Some(ColorScheme::Light),
        ..SystemPreferences::default()
    });
}

fn every_window_command() -> Vec<WindowCommand> {
    let mut commands = vec![
        WindowCommand::Drag,
        WindowCommand::SetDecorations(false),
        WindowCommand::Minimize,
        WindowCommand::ToggleMaximize,
        WindowCommand::SetMaximized(true),
        WindowCommand::Close,
        WindowCommand::SetTitle("Título".into()),
        WindowCommand::Focus,
        WindowCommand::SetCursor(Cursor::Grabbing),
        WindowCommand::Navigate(HistoryUpdate {
            step: HistoryStep::Push(2),
            history: history(),
        }),
        WindowCommand::Navigate(HistoryUpdate {
            step: HistoryStep::Replace,
            history: Vec::new(),
        }),
        WindowCommand::Navigate(HistoryUpdate {
            step: HistoryStep::Back(1),
            history: history(),
        }),
    ];
    commands.extend(ResizeEdge::ALL.map(WindowCommand::DragResize));
    commands
}

#[test]
fn every_window_command_round_trips() {
    for command in every_window_command() {
        round_trip(command);
    }
}

fn window_configs() -> [WindowConfig; 3] {
    [
        WindowConfig::default(),
        WindowConfig {
            title: "Editor".into(),
            width: 1280,
            height: 720,
            min_size: Some((320, 240)),
            max_size: Some((4096, 2160)),
            is_resizable: false,
            has_decorations: false,
            is_transparent: true,
            fullscreen: FullscreenMode::Borderless,
            position: WindowPosition::At(-10, 20),
            is_always_on_top: true,
        },
        WindowConfig {
            fullscreen: FullscreenMode::Exclusive,
            ..WindowConfig::default()
        },
    ]
}

#[test]
fn a_window_config_round_trips_in_every_shape() {
    for config in window_configs() {
        let read = decode::<WindowConfig>(&encode(&config)).unwrap();
        assert_eq!(format!("{read:?}"), format!("{config:?}"));
    }
}

#[test]
fn the_field_taking_text_and_the_keyboard_convention_round_trip() {
    round_trip(TextInput {
        field: 77,
        caret: Rect::new(10.0, 20.0, 1.0, 18.0),
        secret: true,
    });
    round_trip(Some(TextInput {
        field: 1,
        caret: Rect::new(0.0, 0.0, 0.0, 0.0),
        secret: false,
    }));
    round_trip(None::<TextInput>);
    round_trip(KeyboardConvention::Mac);
    round_trip(KeyboardConvention::Pc);
}

fn node() -> AccessNode {
    AccessNode {
        id: Some(4),
        parent: Some(0),
        role: Role::TreeItem,
        name: "Fila".into(),
        rect: Rect::new(1.0, 2.0, 3.0, 4.0),
        focused: true,
        enabled: false,
        toggled: Some(true),
        value: Some(NumericValue {
            now: 5.0,
            min: 0.0,
            max: 10.0,
        }),
        lang: Some("es".into()),
        url: Some("/docs#intro".into()),
        current: Some(CurrentKind::Location),
        orientation: Some(Orientation::Horizontal),
        expanded: Some(false),
        position: Some(SetPosition {
            level: 2,
            position: 1,
            size: 9,
        }),
        active_descendant: Some(6),
        controls: Some(7),
        has_popup: Some(HasPopup::Menu),
        keyboard_shortcut: Some("Control+Shift+Z".into()),
        sort: Some(SortDirection::Ascending),
        multiselectable: true,
        modal: true,
    }
}

fn audit() -> AccessAudit {
    AccessAudit {
        nodes: vec![
            AccessNode {
                id: None,
                parent: None,
                role: Role::Label,
                name: String::new(),
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                focused: false,
                enabled: true,
                toggled: None,
                value: None,
                lang: None,
                url: None,
                current: None,
                orientation: None,
                expanded: None,
                position: None,
                active_descendant: None,
                controls: None,
                has_popup: None,
                keyboard_shortcut: None,
                sort: None,
                multiselectable: false,
                modal: false,
            },
            node(),
        ],
        violations: AccessRule::ALL
            .into_iter()
            .map(|rule| AccessViolation::new(rule, format!("{rule} found"), None))
            .chain([AccessViolation::new(
                AccessRule::Contrast,
                "low",
                Some(Rect::new(1.0, 1.0, 2.0, 2.0)),
            )])
            .collect(),
    }
}

#[test]
fn an_access_audit_round_trips_with_every_rule() {
    round_trip(audit());
    round_trip(AccessAudit::default());
    round_trip(AccessSeverity::Warning);
    round_trip(AccessSeverity::Error);
}

fn put_each<T: Wire>(bytes: &mut Vec<u8>, values: impl IntoIterator<Item = T>) {
    for value in values {
        bytes.extend(encode(&value));
    }
}

#[test]
fn the_bytes_of_every_platform_value_are_pinned_to_the_wire_version() {
    let mut bytes = Vec::new();
    put_each(&mut bytes, every_event());
    put_each(
        &mut bytes,
        [
            NamedKey::Enter,
            NamedKey::Backspace,
            NamedKey::Escape,
            NamedKey::Tab,
            NamedKey::Delete,
            NamedKey::Home,
            NamedKey::End,
            NamedKey::PageUp,
            NamedKey::PageDown,
            NamedKey::ArrowUp,
            NamedKey::ArrowDown,
            NamedKey::ArrowLeft,
            NamedKey::ArrowRight,
            NamedKey::F1,
            NamedKey::F2,
            NamedKey::F3,
            NamedKey::F4,
            NamedKey::F5,
            NamedKey::F6,
            NamedKey::F7,
            NamedKey::F8,
            NamedKey::F9,
            NamedKey::F10,
            NamedKey::F11,
            NamedKey::F12,
            NamedKey::F13,
            NamedKey::F14,
            NamedKey::F15,
            NamedKey::F16,
            NamedKey::F17,
            NamedKey::F18,
            NamedKey::F19,
            NamedKey::F20,
            NamedKey::F21,
            NamedKey::F22,
            NamedKey::F23,
            NamedKey::F24,
            NamedKey::Space,
            NamedKey::Insert,
            NamedKey::CapsLock,
            NamedKey::ContextMenu,
            NamedKey::Numpad0,
            NamedKey::Numpad1,
            NamedKey::Numpad2,
            NamedKey::Numpad3,
            NamedKey::Numpad4,
            NamedKey::Numpad5,
            NamedKey::Numpad6,
            NamedKey::Numpad7,
            NamedKey::Numpad8,
            NamedKey::Numpad9,
            NamedKey::NumpadAdd,
            NamedKey::NumpadSubtract,
            NamedKey::NumpadMultiply,
            NamedKey::NumpadDivide,
            NamedKey::NumpadDecimal,
            NamedKey::NumpadEnter,
        ],
    );
    put_each(
        &mut bytes,
        [
            Cursor::Default,
            Cursor::Pointer,
            Cursor::Crosshair,
            Cursor::Grab,
            Cursor::Grabbing,
            Cursor::ColResize,
            Cursor::RowResize,
            Cursor::EwResize,
            Cursor::NsResize,
            Cursor::NwseResize,
            Cursor::NeswResize,
            Cursor::Move,
            Cursor::Text,
            Cursor::NotAllowed,
            Cursor::Wait,
        ],
    );
    put_each(
        &mut bytes,
        [
            ResizeEdge::North,
            ResizeEdge::South,
            ResizeEdge::East,
            ResizeEdge::West,
            ResizeEdge::NorthEast,
            ResizeEdge::NorthWest,
            ResizeEdge::SouthEast,
            ResizeEdge::SouthWest,
        ],
    );
    put_each(
        &mut bytes,
        [
            AccessRule::UnnamedControl,
            AccessRule::SymbolOnlyName,
            AccessRule::UnnamedPicture,
            AccessRule::Contrast,
            AccessRule::TargetSize,
            AccessRule::HiddenFocusable,
            AccessRule::FocusOrder,
            AccessRule::MissingLang,
        ],
    );
    put_each(
        &mut bytes,
        [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Auxiliary,
        ],
    );
    put_each(&mut bytes, [ColorScheme::Light, ColorScheme::Dark]);
    put_each(
        &mut bytes,
        [
            FullscreenMode::Disabled,
            FullscreenMode::Borderless,
            FullscreenMode::Exclusive,
        ],
    );
    put_each(
        &mut bytes,
        [KeyboardConvention::Mac, KeyboardConvention::Pc],
    );
    put_each(&mut bytes, [AccessSeverity::Warning, AccessSeverity::Error]);
    put_each(&mut bytes, every_window_command());
    put_each(&mut bytes, window_configs());
    put_each(&mut bytes, [SystemPreferences::default(), preferences()]);
    put_each(
        &mut bytes,
        [
            Some(TextInput {
                field: 77,
                caret: Rect::new(10.0, 20.0, 1.0, 18.0),
                secret: true,
            }),
            None,
        ],
    );
    put_each(&mut bytes, [audit(), AccessAudit::default()]);
    wire_core::assert_golden!("src/wire_golden/platform.hex", &bytes);
}

/// Every desktop's handles cross as themselves, and a handle no host that loads a guest has crosses as none rather than failing the frame.
#[test]
fn raw_handles_cross_as_themselves_or_as_none() {
    use std::num::{NonZeroIsize, NonZeroU32};
    use std::ptr::NonNull;

    use raw_window_handle::{
        AppKitDisplayHandle, AppKitWindowHandle, DrmDisplayHandle, DrmWindowHandle,
        GbmDisplayHandle, GbmWindowHandle, RawDisplayHandle, RawWindowHandle, UiKitWindowHandle,
        WaylandDisplayHandle, WaylandWindowHandle, Win32WindowHandle, WinRtWindowHandle,
        WindowsDisplayHandle, XcbDisplayHandle, XcbWindowHandle, XlibDisplayHandle,
        XlibWindowHandle,
    };

    use crate::guest::RawHandles;

    let pointer = NonNull::<u64>::dangling().cast();
    let mut xlib = XlibWindowHandle::new(42);
    xlib.visual_id = 7;
    let mut xcb = XcbWindowHandle::new(NonZeroU32::new(42).unwrap());
    xcb.visual_id = NonZeroU32::new(7);
    let mut win32 = Win32WindowHandle::new(NonZeroIsize::new(-3).unwrap());
    win32.hinstance = NonZeroIsize::new(9);
    let windows = [
        None,
        Some(RawWindowHandle::AppKit(AppKitWindowHandle::new(pointer))),
        Some(RawWindowHandle::Xlib(xlib)),
        Some(RawWindowHandle::Xcb(xcb)),
        Some(RawWindowHandle::Xcb(XcbWindowHandle::new(
            NonZeroU32::new(1).unwrap(),
        ))),
        Some(RawWindowHandle::Wayland(WaylandWindowHandle::new(pointer))),
        Some(RawWindowHandle::Drm(DrmWindowHandle::new(3))),
        Some(RawWindowHandle::Gbm(GbmWindowHandle::new(pointer))),
        Some(RawWindowHandle::Win32(win32)),
        Some(RawWindowHandle::WinRt(WinRtWindowHandle::new(pointer))),
    ];
    let displays = [
        None,
        Some(RawDisplayHandle::AppKit(AppKitDisplayHandle::new())),
        Some(RawDisplayHandle::Xlib(XlibDisplayHandle::new(
            Some(pointer),
            1,
        ))),
        Some(RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0))),
        Some(RawDisplayHandle::Xcb(XcbDisplayHandle::new(
            Some(pointer),
            2,
        ))),
        Some(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            pointer,
        ))),
        Some(RawDisplayHandle::Drm(DrmDisplayHandle::new(5))),
        Some(RawDisplayHandle::Gbm(GbmDisplayHandle::new(pointer))),
        Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
    ];
    for window in windows {
        for display in displays {
            round_trip(RawHandles { window, display });
        }
    }

    let phone = RawHandles {
        window: Some(RawWindowHandle::UiKit(UiKitWindowHandle::new(pointer))),
        display: None,
    };
    assert_eq!(
        decode::<RawHandles>(&encode(&phone)),
        Ok(RawHandles::default())
    );
}
