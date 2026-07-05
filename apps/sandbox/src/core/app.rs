use crate::core::theme::theme;
use rsx::{
    AlignItems, App, AvailableSpace, BorderRadius, Button, ButtonStyle, ClippedItem, Color, Component, Container,
    Event, EventResult, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, NodeId, NodeVec,
    Rect, RectStyle, RenderNode, RwSignal, ShapeStyle, SizeDimension, StyledContainer, Text,
    TextStyle, WidgetCtx, compute_layout, mark_dirty, new_container, new_leaf, set_display, signal,
};

/// Width of the navigation rail / drawer, in px. Kept in sync with the `width:` on `sidebar.rsx`'s root.
const SIDEBAR_W: f32 = 248.0;
/// Height of the mobile top bar (holds the hamburger), in px.
const TOPBAR_H: f32 = 52.0;
/// Below this logical window width the rail collapses into a hamburger drawer.
const MOBILE_BREAKPOINT: f32 = 600.0;

/// Sidebar nav labels, in the same order `build_content` stacks the sections. The index is the section id.
const SECTIONS: [&str; 17] = [
    "Overview",
    "Layout",
    "Sizing & grid",
    "Typography",
    "Color & theme",
    "Boxes & borders",
    "Gradients",
    "Shadows",
    "Opacity & layers",
    "Images",
    "SVG",
    "Paths",
    "Transforms",
    "Buttons",
    "Reactivity",
    "Transitions",
    "Motion",
];

/// Builds every section once and returns the content pane plus each section's layout node, so the
/// shell can show only the selected one by toggling the others' `display`.
fn build_content(ctx: &mut WidgetCtx) -> Result<(Box<dyn LayoutItem>, Vec<NodeId>), LayoutError> {
    let sections: Vec<Box<dyn LayoutItem>> = vec![
        crate::features_overview(ctx)?,
        crate::features_layout(ctx)?,
        crate::features_sizing(ctx)?,
        crate::features_typography(ctx)?,
        crate::features_color(ctx)?,
        crate::features_boxes(ctx)?,
        crate::features_gradients(ctx)?,
        crate::features_shadows(ctx)?,
        crate::features_opacity(ctx)?,
        crate::features_images(ctx)?,
        crate::features_svg(ctx)?,
        crate::features_paths(ctx)?,
        crate::features_transforms(ctx)?,
        crate::features_buttons(ctx)?,
        crate::features_reactivity(ctx)?,
        crate::features_transitions(ctx)?,
        crate::features_motion(ctx)?,
    ];
    let section_nodes: Vec<NodeId> = sections.iter().map(|s| s.layout_node()).collect();
    // Clip each section to its own rect so a hidden one (zero rect via display:none) draws nothing —
    // robust against stale descendant rects and Canvas art that paints at fixed coordinates.
    let sections: Vec<Box<dyn LayoutItem>> = sections
        .into_iter()
        .map(|s| Box::new(ClippedItem::new(ctx, s)) as Box<dyn LayoutItem>)
        .collect();
    // Reading column: fills the width it is given but never past a legible line length.
    let column = Container::new(
        ctx,
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
        ctx,
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::CENTER),
        vec![Box::new(column)],
    )?;
    Ok((Box::new(outer), section_nodes))
}

/// Flat list-item look for a nav button: transparent (blends with the rail) until hovered, filled when it is the active section.
fn nav_button_style(active: bool) -> ButtonStyle {
    let t = theme();
    let radius = BorderRadius::all(8.0);
    if active {
        ButtonStyle {
            rect: RectStyle::default().with_fill(t.primary).with_radius(radius),
            rect_hover: RectStyle::default().with_fill(t.primary).with_radius(radius),
            text: TextStyle::new(13.0, t.on_primary),
            text_hover: TextStyle::new(13.0, t.on_primary),
        }
    } else {
        ButtonStyle {
            rect: RectStyle::default().with_fill(t.surface_alt).with_radius(radius),
            rect_hover: RectStyle::default().with_fill(t.border).with_radius(radius),
            text: TextStyle::new(13.0, t.ink),
            text_hover: TextStyle::new(13.0, t.ink),
        }
    }
}

/// Contents nav: one full-width button per section that sets `selected`; the active one is highlighted.
fn build_nav(
    ctx: &mut WidgetCtx,
    selected: RwSignal<usize>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut buttons: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(SECTIONS.len());
    for (i, title) in SECTIONS.iter().enumerate() {
        let on_style = selected.clone();
        let on_click = selected.clone();
        let btn = Button::new(ctx, *title)?
            .style(move || nav_button_style(on_style.get() == i))
            .on_click(move || on_click.set(i));
        buttons.push(Box::new(btn));
    }
    let list = Container::new(ctx, LayoutStyle::new().flex_column().gap(3.0), buttons)?;
    let label = Text::single_line(
        ctx,
        || "CONTENTS".to_string(),
        || TextStyle::new(11.0, theme().muted),
    )?;
    Ok(Box::new(Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![Box::new(label), Box::new(list)],
    )?))
}

/// Full sidebar: the `.rsx` header + theme switcher, then the Rust-built section nav.
fn build_sidebar(
    ctx: &mut WidgetCtx,
    selected: RwSignal<usize>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let header_theme = crate::core_sidebar(ctx)?;
    let nav = build_nav(ctx, selected)?;
    Ok(Box::new(Container::new(
        ctx,
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
    ctx: &mut WidgetCtx,
    menu_open: RwSignal<bool>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let toggle = menu_open.clone();
    let burger = Button::with_layout(ctx, "\u{2630}", LayoutStyle::new().width(40.0).height(40.0))?
        .style(|| {
            let t = theme();
            ButtonStyle {
                rect: RectStyle::default()
                    .with_fill(t.surface)
                    .with_radius(BorderRadius::all(8.0)),
                rect_hover: RectStyle::default()
                    .with_fill(t.border)
                    .with_radius(BorderRadius::all(8.0)),
                text: TextStyle::new(20.0, t.ink),
                text_hover: TextStyle::new(20.0, t.ink),
            }
        })
        .on_click(move || {
            let open = toggle.peek();
            toggle.set(!open);
        });
    let logo = Text::single_line(
        ctx,
        || "\u{25b2} rsx".to_string(),
        || TextStyle::new(18.0, theme().ink),
    )?;
    let bar = StyledContainer::new(
        ctx,
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
    ctx: WidgetCtx,
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
        mut ctx: WidgetCtx,
        sidebar: Box<dyn LayoutItem>,
        content: Box<dyn LayoutItem>,
        section_nodes: Vec<NodeId>,
        selected: RwSignal<usize>,
    ) -> Result<Self, LayoutError> {
        let content_node = content.layout_node();
        let sidebar_content_node = sidebar.layout_node();
        // Start on the first section: hide the rest so the content pane shows only one section at a time.
        for (i, &node) in section_nodes.iter().enumerate() {
            set_display(&mut ctx, node, i == 0);
        }
        // Wrap the sidebar so it scrolls when the nav is taller than the window; laid out as an overlay.
        let sidebar_scroll = LayoutScrollArea::new(
            &mut ctx,
            LayoutStyle::new()
                .width(SIDEBAR_W)
                .height(SizeDimension::Percent(1.0)),
            sidebar,
        )?;
        let sidebar_scroll_node = sidebar_scroll.layout_node();
        let scroll_area = LayoutScrollArea::new(
            &mut ctx,
            LayoutStyle::new().flex_grow(1.0).align_self_stretch(),
            content,
        )?;
        let menu_open = signal(false);
        let topbar = build_topbar(&mut ctx, menu_open.clone())?;
        let topbar_node = topbar.layout_node();
        let (spacer, _) = new_leaf(
            &mut ctx,
            LayoutStyle::new()
                .width(SIDEBAR_W)
                .height(SizeDimension::Percent(1.0)),
        )?;
        // Body row: the spacer reserves the rail on desktop, the scroll area grows into the rest.
        let body = new_container(
            &mut ctx,
            LayoutStyle::new()
                .flex_row()
                .flex_grow(1.0)
                .width(SizeDimension::Percent(1.0)),
            &[spacer, scroll_area.layout_node()],
        )?;
        // Root column: an optional top bar above the body.
        let root = new_container(
            &mut ctx,
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[topbar_node, body],
        )?;
        Ok(Self {
            ctx,
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
            set_display(&mut self.ctx, node, i == sel);
        }
        let content_width = self.scroll_area.viewport_rect().width.max(0.0);
        mark_dirty(&mut self.ctx, self.content_node).ok();
        compute_layout(
            &mut self.ctx,
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
        set_display(&mut self.ctx, self.topbar_node, mobile);
        set_display(&mut self.ctx, self.spacer, !mobile);
        mark_dirty(&mut self.ctx, self.root).ok();
        compute_layout(
            &mut self.ctx,
            self.root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(height),
        )
        .ok();
        // Content measures its height against whatever width the scroll viewport actually got.
        let content_width = self.scroll_area.viewport_rect().width.max(0.0);
        compute_layout(
            &mut self.ctx,
            self.content_node,
            AvailableSpace::Definite(content_width),
            AvailableSpace::MaxContent,
        )
        .ok();
        // Sidebar overlay at the window's left edge: the viewport is a fixed-width, full-height column;
        // its content is measured at natural height so a tall nav overflows into a scroll instead of clipping.
        compute_layout(
            &mut self.ctx,
            self.sidebar_scroll_node,
            AvailableSpace::Definite(SIDEBAR_W),
            AvailableSpace::Definite(height),
        )
        .ok();
        compute_layout(
            &mut self.ctx,
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
        let mut ctx = WidgetCtx::new();
        let selected = signal(0usize);
        let sidebar = build_sidebar(&mut ctx, selected.clone()).expect("sidebar build failed");
        let (content, section_nodes) = build_content(&mut ctx).expect("content build failed");
        let page = ShellPage::new(ctx, sidebar, content, section_nodes, selected)
            .expect("shell layout failed");
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(theme().background)
    }
}
