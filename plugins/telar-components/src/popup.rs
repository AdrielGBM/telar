//! The panel a list opens in beside what it belongs to — a dropdown's under its trigger, a context menu's at a point, a submenu's beside its row — kept inside the surface, and scrolling when it holds more than fits there.

use std::rc::Rc;
use telar::{
    Anchoring, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, NodeId, Rect, RectStyle,
    Size, StyledContainer, box_item, effect,
    focus::{self, FocusId, Role},
};

/// What a panel is placed and sized by, and where the keyboard is while it is open.
pub(crate) struct Popup {
    /// Read whenever the panel is laid out or drawn, so a panel follows an anchor that moves and a surface that is resized.
    pub anchoring: Rc<dyn Fn() -> Anchoring>,
    pub width: f32,
    pub padding: f32,
    /// The row the keyboard is on: kept scrolled into view.
    pub cursor: Rc<dyn Fn() -> Option<NodeId>>,
    pub keyboard: Keyboard,
}

/// Where the keyboard is while a list is open, which is what the widget it belongs to is: the two patterns a list that drops from a control comes in.
pub(crate) enum Keyboard {
    /// A menu's: the list is a menu that takes focus as it opens and points a reader at the row its cursor is on, `pointed`.
    Held {
        pointed: Rc<dyn Fn() -> Option<FocusId>>,
    },
    /// A select's: focus stays on the combobox that opened the list, which names the list as what it controls and points into it itself. The list is a list box of options, announced and no Tab stop.
    Stays,
}

/// An open panel, and what the widget it belongs to needs of it.
pub(crate) struct Opened {
    pub panel: StyledContainer,
    #[cfg(feature = "overlays")]
    pub view: telar::ScrollViewport,
    /// The list's own control: what a menu hands focus to, and what a select's control names as the one it controls.
    pub list: FocusId,
}

/// The area a panel may occupy: the overlay's, which is unbounded until the surface has been laid out.
pub(crate) fn viewport() -> Rect {
    telar::overlay_viewport().unwrap_or_else(|| Rect::new(0.0, 0.0, f32::MAX, f32::MAX))
}

/// A panel holding `rows`, laid out as tall as they are up to the room [`Anchoring::room`] allows and drawn where [`Anchoring::place`] puts it.
///
/// Placed by a transform rather than by layout, so it is drawn on the right side of its anchor in the frame it is first laid out in: which side that is depends on how tall the rows turn out to be, and a layout placed from that would be a frame late. The panel is laid out at the top-left of the box it is in, which has to be one filling the overlay.
///
/// The list scrolls inside it as one control whose rows are not Tab stops: a menu that takes the keyboard or, for a list whose keyboard stays where it opened from, a list box that is only announced.
pub(crate) fn popup(
    spec: Popup,
    rows: Box<dyn LayoutItem>,
    paint: impl Fn(Rect) -> RectStyle + 'static,
) -> Result<Opened, LayoutError> {
    let Popup {
        anchoring,
        width,
        padding,
        cursor,
        keyboard,
    } = spec;
    let mut handed = None;
    let scroll = LayoutScrollArea::new_with(
        LayoutStyle::new().flex_shrink(1.0).min_height(0.0),
        |viewport| {
            handed = Some(viewport);
            Ok(rows)
        },
    )?;
    let view = handed.expect("a scroll area builds its content before it returns");
    let scroll = match keyboard {
        Keyboard::Held { pointed } => scroll
            .control(Role::Menu)
            .focus_style(|_r| RectStyle::default())
            .active_descendant(move || pointed()),
        Keyboard::Stays => scroll.presented(Role::ListBox),
    };
    let list = scroll
        .focus_id()
        .expect("a list that is a control has a focus id");
    {
        let view = view.clone();
        effect(move || {
            view.rect().get();
            if let Some(node) = cursor() {
                view.reveal(node, 0.0);
            }
        });
    }

    let sheet = {
        let anchoring = anchoring.clone();
        move || {
            telar::use_surface_size();
            let room = anchoring().room();
            LayoutStyle::new()
                .absolute()
                .inset_top(0.0)
                .inset_start(0.0)
                .flex_column()
                .width(width)
                .max_height(room.height)
                .padding_all(padding)
        }
    };
    let panel = StyledContainer::new(sheet(), paint, vec![box_item(scroll)])?
        .styled_by(sheet)
        .with_transform(move |laid| {
            let spot = anchoring().place(Size::new(laid.width, laid.height));
            Some([1.0, 0.0, 0.0, 1.0, spot.x - laid.x, spot.y - laid.y])
        })
        // A press on its own padding is not a press away from it. Answering one would make the box a button, so it says it is the group it is.
        .on_press(|| {})
        .role(Role::Group);
    Ok(Opened {
        panel,
        #[cfg(feature = "overlays")]
        view,
        list,
    })
}

/// Where the keyboard was when a popup opened, so that every way of closing it — a pick, Escape, a press away — puts the keyboard back there rather than leaving it nowhere.
///
/// Given back only when the keyboard is still inside the popup or was dropped with it: one that went somewhere else while the popup was up stays where it went.
#[derive(Clone, Default)]
pub(crate) struct Handback(Rc<std::cell::Cell<Option<(focus::Remembered, NodeId)>>>);

impl Handback {
    /// Remembers where focus is as a popup whose content hangs from `content` opens.
    pub(crate) fn remember(&self, content: NodeId) {
        self.0.set(focus::remember().map(|back| (back, content)));
    }

    /// Gives focus back as the popup closes.
    pub(crate) fn give_back(&self) {
        if let Some((back, content)) = self.0.take()
            && focus::left_within(content)
        {
            back.restore();
        }
    }
}
