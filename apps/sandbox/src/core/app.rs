use crate::core::theme::theme;
use rsx::{
    AlignItems, App, AvailableSpace, BorderRadius, ClippedItem, Color, Component, Container, Event,
    EventResult, JustifyContent, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, NodeId,
    NodeVec, Rect, RectStyle, RenderNode, RwSignal, ShapeStyle, SizeDimension, StyledContainer, Text,
    TextStyle, compute_layout, mark_dirty, new_container, new_leaf, reset_layout_runtime,
    set_display, set_overlay_host, signal,
};

/// Width of the navigation rail / drawer, in px. Kept in sync with the `width:` on `sidebar.rsx`'s root.
const SIDEBAR_W: f32 = 248.0;
/// Height of the mobile top bar (holds the hamburger), in px.
const TOPBAR_H: f32 = 52.0;
/// Below this logical window width the rail collapses into a hamburger drawer.
const MOBILE_BREAKPOINT: f32 = 600.0;

/// Builds one doc section into its content pane. Every `.rsx` feature transpiles to a fn of this shape.
type SectionBuild = fn() -> Result<Box<dyn LayoutItem>, LayoutError>;

/// Every doc section — nav label and content builder — in display order. The index is the section id, and
/// this is the single source of truth the sidebar nav and the content pane both derive from: adding or
/// reordering a section is a one-line edit here, not three in sync.
const SECTIONS: &[(&str, SectionBuild)] = &[
    ("Overview", crate::features_overview),
    ("Layout", crate::features_layout),
    ("Sizing & grid", crate::features_sizing),
    ("Typography", crate::features_typography),
    ("Color & theme", crate::features_color),
    ("Boxes & borders", crate::features_boxes),
    ("Gradients", crate::features_gradients),
    ("Shadows", crate::features_shadows),
    ("Opacity & layers", crate::features_opacity),
    ("Images", crate::features_images),
    ("SVG", crate::features_svg),
    ("Paths", crate::features_paths),
    ("Transforms", crate::features_transforms),
    ("Buttons", crate::features_buttons),
    ("Form controls", crate::features_forms),
    ("Sliders", crate::features_sliders),
    ("Text fields", crate::features_text_fields),
    ("Menus & select", crate::features_menus),
    ("Dialogs & overlays", crate::features_dialogs),
    ("Reactivity", crate::features_reactivity),
    ("Transitions", crate::features_transitions),
    ("Motion", crate::features_motion),
];

/// Builds every section once and returns the content pane plus each section's layout node, so the
/// shell can show only the selected one by toggling the others' `display`.
fn build_content() -> Result<(Box<dyn LayoutItem>, Vec<NodeId>), LayoutError> {
    let mut sections: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(SECTIONS.len());
    for (_, build) in SECTIONS {
        sections.push(build()?);
    }
    let section_nodes: Vec<NodeId> = sections.iter().map(|s| s.layout_node()).collect();
    // Clip each section to its own rect so a hidden one (zero rect via display:none) draws nothing —
    // robust against stale descendant rects and Canvas art that paints at fixed coordinates.
    let sections: Vec<Box<dyn LayoutItem>> = sections
        .into_iter()
        .map(|s| Box::new(ClippedItem::new(s)) as Box<dyn LayoutItem>)
        .collect();
    // Reading column: fills the width it is given but never past a legible line length.
    let column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .max_width(960.0)
            .padding_all(32.0)
            .gap(40.0),
        sections,
    )?;
    // Outer wrapper fills the scroll viewport and centers the capped column on wide windows.
    let outer = Container::new(
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::CENTER),
        vec![Box::new(column)],
    )?;
    Ok((Box::new(outer), section_nodes))
}

/// Base paint for a nav item: the active section is filled with the accent; the rest blend into the rail
/// (its `surface_alt` panel), so an inactive item reads as flat until hovered.
fn nav_rect(active: bool) -> RectStyle {
    let t = theme();
    let radius = BorderRadius::all(8.0);
    let fill = if active { t.primary } else { t.surface_alt };
    RectStyle::default().with_fill(fill).with_radius(radius)
}

/// Hover paint for a nav item: the active one keeps the accent; an inactive one lifts to the border tone.
fn nav_rect_hover(active: bool) -> RectStyle {
    let t = theme();
    let radius = BorderRadius::all(8.0);
    let fill = if active { t.primary } else { t.border };
    RectStyle::default().with_fill(fill).with_radius(radius)
}

/// Contents nav: one full-width button per section that sets `selected`; the active one is highlighted.
/// Built on the kernel primitives — a `StyledContainer` (transparent-blending until hover, filled when
/// active) with a centred `Text::auto` label and an `on_press` that selects the section.
fn build_nav(
    selected: RwSignal<usize>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut buttons: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(SECTIONS.len());
    for (i, (title, _)) in SECTIONS.iter().enumerate() {
        let title = *title;
        let on_label = selected.clone();
        let label = Text::auto(
            move || title.to_string(),
            LayoutStyle::new(),
            move || {
                let t = theme();
                let color = if on_label.get() == i { t.on_primary } else { t.ink };
                TextStyle::new(13.0, color)
            },
        )?;
        let on_base = selected.clone();
        let on_hover = selected.clone();
        let on_press = selected.clone();
        // A row: the parent column stretches the item to full width, and `justify_content:center` centres
        // the measured `Text::auto` label within it (a column would collapse the label's stretched cross axis).
        let btn = StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER)
                .padding_horizontal(14.0)
                // Light vertical padding keeps each item near its old height (`Text::auto` already reserves a
                // full line box), so the tall nav still fits the rail and every section stays reachable.
                .padding_vertical(3.0),
            move |_r| nav_rect(on_base.get() == i),
            vec![Box::new(label)],
        )?
        .on_hover_style(move |_r| nav_rect_hover(on_hover.get() == i))
        .on_press(move || on_press.set(i));
        buttons.push(Box::new(btn));
    }
    let list = Container::new(LayoutStyle::new().flex_column().gap(3.0), buttons)?;
    let label = Text::single_line(
        || "CONTENTS".to_string(),
        || TextStyle::new(11.0, theme().muted),
    )?;
    Ok(Box::new(Container::new(
        LayoutStyle::new().flex_column().gap(8.0),
        vec![Box::new(label), Box::new(list)],
    )?))
}

/// Full sidebar: the `.rsx` header + theme switcher, then the Rust-built section nav.
fn build_sidebar(
    selected: RwSignal<usize>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let header_theme = crate::core_sidebar()?;
    let nav = build_nav(selected)?;
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_column()
            .width(SIDEBAR_W)
            .padding_all(20.0)
            .gap(22.0),
        vec![header_theme, nav],
    )?))
}

/// Mobile top bar: a hamburger button (toggles `menu_open`) next to the wordmark. Shown only below the breakpoint.
fn build_topbar(
    menu_open: RwSignal<bool>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let toggle = menu_open.clone();
    let glyph = Text::auto(
        || "\u{2630}".to_string(),
        LayoutStyle::new(),
        || TextStyle::new(20.0, theme().ink),
    )?;
    let burger = StyledContainer::new(
        // Padding content-sizes the icon to ~40x40; a row keeps the measured glyph from collapsing.
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_horizontal(10.0)
            .padding_vertical(6.0),
        |_r| {
            RectStyle::default()
                .with_fill(theme().surface)
                .with_radius(BorderRadius::all(8.0))
        },
        vec![Box::new(glyph)],
    )?
    .on_hover_style(|_r| {
        RectStyle::default()
            .with_fill(theme().border)
            .with_radius(BorderRadius::all(8.0))
    })
    .on_press(move || {
        let open = toggle.peek();
        toggle.set(!open);
    });
    let logo = Text::single_line(
        || "\u{25b2} rsx".to_string(),
        || TextStyle::new(18.0, theme().ink),
    )?;
    let bar = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(SizeDimension::Percent(1.0))
            .height(TOPBAR_H)
            .align_items(AlignItems::CENTER)
            .gap(12.0)
            .padding_all(10.0),
        |_r| RectStyle::default().with_fill(theme().surface_alt),
        vec![Box::new(burger), Box::new(logo)],
    )?;
    Ok(Box::new(bar))
}

/// Responsive application shell. Desktop (≥ breakpoint): the sidebar is a fixed rail whose width is
/// reserved by an empty `spacer` and painted as an always-on overlay at the window's left edge.
/// Mobile (< breakpoint): the rail collapses — a top bar appears, and the sidebar becomes a drawer
/// that slides over a dimming scrim when `menu_open` is set. The sidebar is laid out on its own so it
/// can overlay content in either mode; the scroll viewport is recomputed on every resize.
struct ShellPage {
    root: NodeId,
    content_node: NodeId,
    // Empty leaf that reserves the rail's width on desktop; hidden on mobile so content spans full width.
    spacer: NodeId,
    topbar_node: NodeId,
    topbar: Box<dyn LayoutItem>,
    // The sidebar overlay is itself a scroll area so a tall nav can scroll on short screens.
    sidebar_scroll: LayoutScrollArea,
    sidebar_scroll_node: NodeId,
    sidebar_content_node: NodeId,
    scroll_area: LayoutScrollArea,
    menu_open: RwSignal<bool>,
    // Which section is shown; the sidebar sets it, `apply_selection` reflects it into the content pane.
    selected: RwSignal<usize>,
    current_section: usize,
    // Layout node of each section, toggled via `display` so only the selected one is laid out and drawn.
    section_nodes: Vec<NodeId>,
    mobile: bool,
    win_w: f32,
    win_h: f32,
    // Last pointer position, tracked so coordinate-less Scrolled events route to whichever pane the pointer is over.
    ptr_x: f32,
    ptr_y: f32,
}

impl ShellPage {
    fn new(
        sidebar: Box<dyn LayoutItem>,
        content: Box<dyn LayoutItem>,
        section_nodes: Vec<NodeId>,
        selected: RwSignal<usize>,
    ) -> Result<Self, LayoutError> {
        let content_node = content.layout_node();
        let sidebar_content_node = sidebar.layout_node();
        // Start on the first section: hide the rest so the content pane shows only one section at a time.
        for (i, &node) in section_nodes.iter().enumerate() {
            set_display(node, i == 0);
        }
        // Wrap the sidebar so it scrolls when the nav is taller than the window; laid out as an overlay.
        let sidebar_scroll = LayoutScrollArea::new(
            LayoutStyle::new()
                .width(SIDEBAR_W)
                .height(SizeDimension::Percent(1.0)),
            sidebar,
        )?;
        let sidebar_scroll_node = sidebar_scroll.layout_node();
        let scroll_area = LayoutScrollArea::new(
            LayoutStyle::new().flex_grow(1.0).align_self_stretch(),
            content,
        )?;
        let menu_open = signal(false);
        let topbar = build_topbar(menu_open.clone())?;
        let topbar_node = topbar.layout_node();
        let (spacer, _) = new_leaf(
            LayoutStyle::new()
                .width(SIDEBAR_W)
                .height(SizeDimension::Percent(1.0)),
        )?;
        // Body row: the spacer reserves the rail on desktop, the scroll area grows into the rest.
        let body = new_container(
            LayoutStyle::new()
                .flex_row()
                .flex_grow(1.0)
                .width(SizeDimension::Percent(1.0)),
            &[spacer, scroll_area.layout_node()],
        )?;
        // Root column: an optional top bar above the body.
        let root = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[topbar_node, body],
        )?;
        Ok(Self {
            root,
            content_node,
            spacer,
            topbar_node,
            topbar,
            sidebar_scroll,
            sidebar_scroll_node,
            sidebar_content_node,
            scroll_area,
            menu_open,
            selected,
            current_section: 0,
            section_nodes,
            mobile: false,
            win_w: 0.0,
            win_h: 0.0,
            // Defaults to the content pane so a scroll before the first pointer event does the expected thing.
            ptr_x: f32::MAX,
            ptr_y: 0.0,
        })
    }

    /// If the sidebar changed `selected`, show that section (hiding the rest), relayout the content, and
    /// scroll back to the top. On mobile it also closes the drawer so the chosen section is revealed.
    fn apply_selection(&mut self) {
        let sel = self
            .selected
            .get()
            .min(self.section_nodes.len().saturating_sub(1));
        if sel == self.current_section {
            return;
        }
        self.current_section = sel;
        for (i, &node) in self.section_nodes.iter().enumerate() {
            set_display(node, i == sel);
        }
        let content_width = self.scroll_area.viewport_rect().width.max(0.0);
        mark_dirty(self.content_node).ok();
        compute_layout(
            self.content_node,
            AvailableSpace::Definite(content_width),
            AvailableSpace::MaxContent,
        )
        .ok();
        self.scroll_area.scroll_to_top();
        self.scroll_area.clamp_scroll();
        if self.mobile {
            self.menu_open.set(false);
        }
    }

    fn relayout(&mut self, width: f32, height: f32) {
        self.win_w = width;
        self.win_h = height;
        let mobile = width < MOBILE_BREAKPOINT;
        self.mobile = mobile;
        // Leaving mobile turns the drawer back into the always-on rail; drop any stale open state.
        if !mobile {
            self.menu_open.set(false);
        }
        // Top bar only on mobile; the desktop rail is reserved by the spacer instead.
        set_display(self.topbar_node, mobile);
        set_display(self.spacer, !mobile);
        mark_dirty(self.root).ok();
        compute_layout(
            self.root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(height),
        )
        .ok();
        // Pin the window-spanning root as the overlay host: the sidebar/content are computed as their own
        // parent-less roots below, and auto-detection would otherwise make the last one (the 248px sidebar)
        // the host — so modals/drawers/menus would portal over the sidebar instead of the viewport.
        set_overlay_host(self.root);
        // Content measures its height against whatever width the scroll viewport actually got.
        let content_width = self.scroll_area.viewport_rect().width.max(0.0);
        compute_layout(
            self.content_node,
            AvailableSpace::Definite(content_width),
            AvailableSpace::MaxContent,
        )
        .ok();
        // Sidebar overlay at the window's left edge: the viewport is a fixed-width, full-height column;
        // its content is measured at natural height so a tall nav overflows into a scroll instead of clipping.
        compute_layout(
            self.sidebar_scroll_node,
            AvailableSpace::Definite(SIDEBAR_W),
            AvailableSpace::Definite(height),
        )
        .ok();
        compute_layout(
            self.sidebar_content_node,
            AvailableSpace::Definite(SIDEBAR_W),
            AvailableSpace::MaxContent,
        )
        .ok();
        self.sidebar_scroll.clamp_scroll();
        self.scroll_area.clamp_scroll();
    }
}

impl Component for ShellPage {
    fn view(&self) -> RenderNode {
        let open = self.menu_open.get();
        let mut nodes = vec![self.scroll_area.view()];
        if self.mobile {
            nodes.push(self.topbar.view());
        }
        if !self.mobile || open {
            let mut overlay = Vec::new();
            // On mobile the drawer is modal: dim the content behind it. On desktop the sidebar is always the rail.
            if self.mobile && open {
                overlay.push(RenderNode::rect(
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: self.win_w,
                        height: self.win_h,
                    },
                    RectStyle::default().with_fill(Color::rgba(0.0, 0.0, 0.0, 0.45)),
                ));
            }
            // Paint the panel background first so it fills the full height even when the nav is shorter than the window.
            overlay.push(RenderNode::rect(
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: SIDEBAR_W,
                    height: self.win_h,
                },
                RectStyle::default().with_fill(theme().surface_alt),
            ));
            overlay.push(self.sidebar_scroll.view());
            // Wrap the overlay in a clip: besides bounding it to the window, the clip is a structural
            // boundary that stops the hardware batcher (merge_opaque_batches) from reordering the top
            // bar's text above the drawer background — otherwise the hamburger icon floats over the drawer.
            nodes.push(RenderNode::Clip {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: self.win_w,
                    height: self.win_h,
                },
                radius: BorderRadius::zero(),
                children: NodeVec::collect(overlay),
            });
        }
        RenderNode::group(nodes)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            self.relayout(*width as f32, *height as f32);
            return EventResult::Handled;
        }
        match event {
            Event::PointerMoved { x, y, .. } | Event::PointerPressed { x, y, .. } => {
                self.ptr_x = *x as f32;
                self.ptr_y = *y as f32;
            }
            _ => {}
        }
        // A Scrolled event carries no coordinates, so route it by where the pointer last was.
        let over_sidebar = self.ptr_x < SIDEBAR_W;

        if self.mobile {
            if self.menu_open.get() {
                if let Event::Scrolled { .. } = event {
                    // Only the drawer scrolls while it is open; a scroll over the scrim is swallowed.
                    if over_sidebar {
                        self.sidebar_scroll.on_event(event);
                    }
                    return EventResult::Handled;
                }
                // Drawer open: the sidebar (theme + nav buttons) hit-tests first; a press on the scrim (right of the drawer) closes it.
                if self.sidebar_scroll.on_event(event) == EventResult::Handled {
                    self.apply_selection();
                    return EventResult::Handled;
                }
                if let Event::PointerPressed { x, .. } = event {
                    if *x as f32 >= SIDEBAR_W {
                        self.menu_open.set(false);
                    }
                }
                // Swallow everything else so the dimmed content underneath stays inert.
                return EventResult::Handled;
            }
            // Drawer closed: the hamburger toggles it, otherwise the content scrolls.
            if self.topbar.on_event(event) == EventResult::Handled {
                return EventResult::Handled;
            }
            return self.scroll_area.on_event(event);
        }
        // Desktop: scroll goes to whichever pane the pointer is over; the rail never eats the content's scroll.
        if let Event::Scrolled { .. } = event {
            return if over_sidebar {
                self.sidebar_scroll.on_event(event)
            } else {
                self.scroll_area.on_event(event)
            };
        }
        // Other events: the sidebar rail hit-tests first (by coords), then the scroll area.
        if self.sidebar_scroll.on_event(event) == EventResult::Handled {
            self.apply_selection();
            return EventResult::Handled;
        }
        self.scroll_area.on_event(event)
    }
}

pub struct SandboxRoot;

impl App for SandboxRoot {
    fn root(&self) -> Box<dyn rsx::Component> {
        reset_layout_runtime();
        let selected = signal(0usize);
        let sidebar = build_sidebar(selected.clone()).expect("sidebar build failed");
        let (content, section_nodes) = build_content().expect("content build failed");
        let page = ShellPage::new(sidebar, content, section_nodes, selected)
            .expect("shell layout failed");
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(theme().background)
    }
}
