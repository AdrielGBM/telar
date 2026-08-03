use crate::core::theme::theme;
use telar::{
    AlignItems, App, AvailableSpace, BorderRadius, Color, Component, Container, Event, EventResult,
    JustifyContent, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, NavPage, NavTransition,
    Navigator, NodeId, NodeVec, PagePolicy, Rect, RectStyle, RenderNode, RwSignal, ShapeStyle,
    SizeDimension, StyledContainer, TabHost, TabStacks, Text, TextStyle, compute_layout,
    hot_signal, mark_dirty, new_container, new_leaf, reset_layout_runtime, set_display,
    set_overlay_host, signal, transform_pointer, use_direction, use_dismiss_depth,
};

/// Width of the navigation rail / drawer, in px. Kept in sync with the `width:` on `sidebar.rsx`'s root.
const SIDEBAR_W: f32 = 248.0;
/// Height of the mobile top bar (holds the hamburger), in px.
const TOPBAR_H: f32 = 52.0;
/// Below this logical window width the rail collapses into a hamburger drawer.
const MOBILE_BREAKPOINT: f32 = 600.0;

/// Builds one doc section into its content pane. Every `.rsx` feature transpiles to a fn of this shape.
type SectionBuild = fn() -> Result<Box<dyn LayoutItem>, LayoutError>;

/// One doc section: its nav label, its content builder, and the `.rsx` file behind it (name plus the source
/// text itself, baked in at compile time for the source detail page).
struct SectionDef {
    title: &'static str,
    build: SectionBuild,
    file: &'static str,
    source: &'static str,
}

/// A destination *within* a section. The section itself is not part of this: each rail item is a tab with its
/// own stack, so the route only has to say how deep into that section you are — the overview a reader lands
/// on, or the source listing pushed over it. Back returns to the overview at the scroll position it had.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
enum SectionRoute {
    Overview,
    Source,
}

/// Builds the [`SECTIONS`] table, baking each entry's `.rsx` source in beside its builder so a section stays a
/// one-line edit. `include_str!` needs a literal path, which is why the file name is spelled out per entry.
macro_rules! sections {
    ($(($title:literal, $build:path, $file:literal)),* $(,)?) => {
        &[$(SectionDef {
            title: $title,
            build: $build,
            file: $file,
            source: include_str!(concat!("../features/", $file)),
        }),*]
    };
}

/// Every doc section in display order. The index is the section id, and this is the single source of truth the
/// sidebar nav and the content pane both derive from: adding or reordering a section is a one-line edit here,
/// not three in sync.
const SECTIONS: &[SectionDef] = sections![
    ("Overview", crate::features_overview, "overview.rsx"),
    ("Layout", crate::features_layout, "layout.rsx"),
    ("Sizing & grid", crate::features_sizing, "sizing.rsx"),
    ("Typography", crate::features_typography, "typography.rsx"),
    ("Color & theme", crate::features_color, "color.rsx"),
    ("Boxes & borders", crate::features_boxes, "boxes.rsx"),
    ("Gradients", crate::features_gradients, "gradients.rsx"),
    ("Shadows", crate::features_shadows, "shadows.rsx"),
    ("Opacity & layers", crate::features_opacity, "opacity.rsx"),
    ("Images", crate::features_images, "images.rsx"),
    ("SVG", crate::features_svg, "svg.rsx"),
    ("Paths", crate::features_paths, "paths.rsx"),
    ("Transforms", crate::features_transforms, "transforms.rsx"),
    ("Buttons", crate::features_buttons, "buttons.rsx"),
    ("Form controls", crate::features_forms, "forms.rsx"),
    ("Sliders", crate::features_sliders, "sliders.rsx"),
    (
        "Text fields",
        crate::features_text_fields,
        "text_fields.rsx"
    ),
    ("Stepper", crate::features_steppers, "steppers.rsx"),
    (
        "Progress & spinner",
        crate::features_indicators,
        "indicators.rsx"
    ),
    (
        "Tabs & accordion",
        crate::features_navigation,
        "navigation.rsx"
    ),
    ("Badges & chips", crate::features_pills, "pills.rsx"),
    ("Menus & select", crate::features_menus, "menus.rsx"),
    ("Dialogs & overlays", crate::features_dialogs, "dialogs.rsx"),
    ("Reactivity", crate::features_reactivity, "reactivity.rsx"),
    (
        "Transitions",
        crate::features_transitions,
        "transitions.rsx"
    ),
    ("Motion", crate::features_motion, "motion.rsx"),
];

/// A restored hot-reload stack (or a deep link) can name a section that no longer exists; clamp rather than panic.
fn section_def(section: usize) -> &'static SectionDef {
    &SECTIONS[section.min(SECTIONS.len() - 1)]
}

/// One doc section as a navigable page: the section's content in a reading column, inside its own scroll
/// viewport. Each page scrolling itself is what lets a section keep its reading position while another is
/// on screen — navigating back returns to where you were, as a page stack should.
struct SectionPage {
    scroll: LayoutScrollArea,
}

impl SectionPage {
    fn new(nav: Navigator<SectionRoute>, section: usize) -> Result<Self, LayoutError> {
        let def = section_def(section);
        // Reading column: fills the width it is given but never past a legible line length.
        let column = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .max_width(960.0)
                .padding_all(32.0)
                .gap(40.0),
            vec![(def.build)()?, build_source_link(nav, section)?],
        )?;
        // Outer wrapper fills the scroll viewport and centers the capped column on wide windows.
        let centered = Container::new(
            LayoutStyle::new()
                .flex_column()
                .align_items(AlignItems::CENTER),
            vec![Box::new(column)],
        )?;
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new().flex_grow(1.0).align_self_stretch(),
            Box::new(centered),
        )?;
        Ok(Self { scroll })
    }
}

impl Component for SectionPage {
    fn view(&self) -> RenderNode {
        self.scroll.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.scroll.on_event(event)
    }
}

impl LayoutItem for SectionPage {
    fn layout_node(&self) -> NodeId {
        self.scroll.layout_node()
    }
}

impl NavPage for SectionPage {
    fn on_relayout(&mut self) {
        self.scroll.clamp_scroll();
    }
}

/// Footer of a section page: pushes that section's `.rsx` source as a detail page. A push rather than an
/// overlay because the listing is long — its scroll position is state the stack should remember, and Back is
/// the way out.
fn build_source_link(
    nav: Navigator<SectionRoute>,
    section: usize,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let file = section_def(section).file;
    let label = Text::auto(
        move || format!("View source \u{2192} {file}"),
        LayoutStyle::new(),
        || TextStyle::new(13.0, theme().primary),
    )?;
    let btn = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .padding_horizontal(14.0)
            .padding_vertical(6.0),
        |_r| {
            RectStyle::default()
                .with_fill(theme().surface_alt)
                .with_radius(BorderRadius::all(8.0))
        },
        vec![Box::new(label)],
    )?
    .on_hover_style(|_r| {
        RectStyle::default()
            .with_fill(theme().border)
            .with_radius(BorderRadius::all(8.0))
    })
    .on_press(move || nav.push(SectionRoute::Source));
    // A row so the button hugs its label instead of stretching across the reading column.
    Ok(Box::new(Container::new(
        LayoutStyle::new().flex_row(),
        vec![Box::new(btn)],
    )?))
}

/// The `.rsx` source behind a section, as a pushed detail page: the file name above the listing, in its own
/// scroll viewport.
///
/// The listing is one wrapped text block rather than a [`LineGutter`](telar::LineGutter) column: a gutter numbers
/// *logical* lines, and with no monospace or no-wrap in `TextStyle` a long line soft-wraps, which would slide
/// the numbers out of step with the code.
struct SourcePage {
    scroll: LayoutScrollArea,
}

impl SourcePage {
    fn new(section: usize) -> Result<Self, LayoutError> {
        let def = section_def(section);
        let (file, source) = (def.file, def.source);
        let heading = Text::auto(
            move || file.to_string(),
            LayoutStyle::new(),
            || TextStyle::new(20.0, theme().ink).with_weight(700),
        )?;
        let listing = Text::auto(
            move || source.to_string(),
            LayoutStyle::new(),
            || TextStyle::new(12.5, theme().ink).with_line_height(1.6),
        )?;
        let panel = StyledContainer::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .padding_all(20.0),
            |_r| {
                RectStyle::default()
                    .with_fill(theme().surface_alt)
                    .with_radius(BorderRadius::all(10.0))
            },
            vec![Box::new(listing)],
        )?;
        let column = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .max_width(960.0)
                .padding_all(32.0)
                .gap(20.0),
            vec![Box::new(heading), Box::new(panel)],
        )?;
        let centered = Container::new(
            LayoutStyle::new()
                .flex_column()
                .align_items(AlignItems::CENTER),
            vec![Box::new(column)],
        )?;
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new().flex_grow(1.0).align_self_stretch(),
            Box::new(centered),
        )?;
        Ok(Self { scroll })
    }
}

impl Component for SourcePage {
    fn view(&self) -> RenderNode {
        self.scroll.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.scroll.on_event(event)
    }
}

impl LayoutItem for SourcePage {
    fn layout_node(&self) -> NodeId {
        self.scroll.layout_node()
    }
}

impl NavPage for SourcePage {
    fn on_relayout(&mut self) {
        self.scroll.clamp_scroll();
    }
}

/// Builds whichever page a route names inside `section` — the [`TabHost`]'s factory, called on a route's first
/// visit to that section's own stack. The stack handed to the overview is that section's, so its source link
/// pushes onto the section it belongs to rather than onto whatever tab happens to be active.
fn build_page(
    stacks: &TabStacks<usize, SectionRoute>,
    section: usize,
    route: SectionRoute,
) -> Result<Box<dyn NavPage>, LayoutError> {
    Ok(match route {
        SectionRoute::Overview => {
            let nav = stacks
                .navigator_for(&section)
                .expect("every section declares a stack");
            Box::new(SectionPage::new(nav, section)?) as Box<dyn NavPage>
        }
        SectionRoute::Source => Box::new(SourcePage::new(section)?),
    })
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

/// Back control: closes an open dialog if there is one, else pops the current section's own stack (out of a
/// source listing, back to its overview), else returns to the section read before this one. Always present,
/// but it dims to `muted` once there is nothing left to go back to, so it reads as unavailable without the
/// layout shifting when history appears.
fn build_back(stacks: TabStacks<usize, SectionRoute>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // Live when there is either a dialog to close or a page to pop; both reads are reactive, so the control lights up the moment a modal opens even at the root of a section's stack.
    let live =
        move |stacks: &TabStacks<usize, SectionRoute>| use_dismiss_depth() > 0 || stacks.can_pop();
    let on_label = stacks.clone();
    let label = Text::auto(
        || "\u{2190} Back".to_string(),
        LayoutStyle::new(),
        move || {
            let t = theme();
            let color = if live(&on_label) { t.ink } else { t.muted };
            TextStyle::new(13.0, color)
        },
    )?;
    let on_hover = stacks.clone();
    let on_press = stacks.clone();
    let btn = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .padding_horizontal(14.0)
            .padding_vertical(3.0),
        |_r| {
            RectStyle::default()
                .with_fill(theme().surface_alt)
                .with_radius(BorderRadius::all(8.0))
        },
        vec![Box::new(label)],
    )?
    // Only lifts on hover when there is something to go back to, so the dimmed state stays inert.
    .on_hover_style(move |_r| {
        nav_rect(false).with_fill(if live(&on_hover) {
            theme().border
        } else {
            theme().surface_alt
        })
    })
    .on_press(move || {
        on_press.back();
    });
    Ok(Box::new(btn))
}

/// Contents nav: one full-width button per section, above a back control. The active one is highlighted.
/// Each item is a *tab*, not a history entry: selecting one switches to that section's own stack, leaving the
/// one you came from standing exactly where it was — a source listing still open, scrolled where you left it.
/// Pressing the section you are already reading pops it back to its overview, so an item is never inert.
fn build_nav(stacks: TabStacks<usize, SectionRoute>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut buttons: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(SECTIONS.len());
    for (i, def) in SECTIONS.iter().enumerate() {
        let title = def.title;
        let on_label = stacks.clone();
        let label = Text::auto(
            move || title.to_string(),
            LayoutStyle::new(),
            move || {
                let t = theme();
                let color = if on_label.active() == i {
                    t.on_primary
                } else {
                    t.ink
                };
                TextStyle::new(13.0, color)
            },
        )?;
        let on_base = stacks.clone();
        let on_hover = stacks.clone();
        let on_press = stacks.clone();
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
            move |_r| nav_rect(on_base.active() == i),
            vec![Box::new(label)],
        )?
        .on_hover_style(move |_r| nav_rect_hover(on_hover.active() == i))
        .on_press(move || on_press.select(i));
        buttons.push(Box::new(btn));
    }
    let list = Container::new(LayoutStyle::new().flex_column().gap(3.0), buttons)?;
    let label = Text::single_line(
        || "CONTENTS".to_string(),
        || TextStyle::new(11.0, theme().muted),
    )?;
    Ok(Box::new(Container::new(
        LayoutStyle::new().flex_column().gap(8.0),
        vec![build_back(stacks)?, Box::new(label), Box::new(list)],
    )?))
}

/// Full sidebar: the `.rsx` header + theme switcher, then the Rust-built section nav.
fn build_sidebar(
    stacks: TabStacks<usize, SectionRoute>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let header_theme = crate::core_sidebar()?;
    let nav = build_nav(stacks)?;
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
fn build_topbar(menu_open: RwSignal<bool>) -> Result<Box<dyn LayoutItem>, LayoutError> {
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
/// can overlay content in either mode.
///
/// The content pane is a [`TabHost`]: every rail item is a tab with its own page stack, built the first time
/// it is visited, so opening the app costs one section rather than all 26.
struct ShellPage {
    root: NodeId,
    // Empty leaf that reserves the rail's width on desktop; hidden on mobile so content spans full width.
    spacer: NodeId,
    topbar_node: NodeId,
    topbar: Box<dyn LayoutItem>,
    // The sidebar overlay is itself a scroll area so a tall nav can scroll on short screens.
    sidebar_scroll: LayoutScrollArea,
    sidebar_scroll_node: NodeId,
    sidebar_content_node: NodeId,
    nav_host: TabHost<usize, SectionRoute>,
    menu_open: RwSignal<bool>,
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
        stacks: TabStacks<usize, SectionRoute>,
    ) -> Result<Self, LayoutError> {
        let sidebar_content_node = sidebar.layout_node();
        let factory_stacks = stacks.clone();
        let nav_host = TabHost::new(stacks, move |section: &usize, route: &SectionRoute| {
            build_page(&factory_stacks, *section, *route)
        })?
        // Every page is a stack entry now: a section survives being left because its *stack* stays alive while another tab is on screen, not because the host pins it by route. So a source listing is fresh on each push and released on the way back, and two visits to the same file never share a scroll.
        .with_policy(PagePolicy::Transient)
        .with_transition(NavTransition::Fade)
        // The rail reads as a table of contents rather than a tab bar, so switching sections gets the same fade as drilling into one. A real tab bar would leave this off and swap instantly.
        .with_tab_transition(NavTransition::Fade);
        // Wrap the sidebar so it scrolls when the nav is taller than the window; laid out as an overlay.
        let sidebar_scroll = LayoutScrollArea::new(
            LayoutStyle::new()
                .width(SIDEBAR_W)
                .height(SizeDimension::Percent(1.0)),
            sidebar,
        )?;
        let sidebar_scroll_node = sidebar_scroll.layout_node();
        let menu_open = signal(false);
        let topbar = build_topbar(menu_open.clone())?;
        let topbar_node = topbar.layout_node();
        let (spacer, _) = new_leaf(
            LayoutStyle::new()
                .width(SIDEBAR_W)
                .height(SizeDimension::Percent(1.0)),
        )?;
        // The host's own container is `width: 100%`, which as a flex-basis beside the 248px spacer would
        // overflow the row; this wrapper gives it the remaining width to be 100% of instead.
        let content = new_container(
            LayoutStyle::new().flex_row().flex_grow(1.0),
            &[nav_host.layout_node()],
        )?;
        // Body row: the spacer reserves the rail on desktop, the content grows into the rest.
        let body = new_container(
            LayoutStyle::new()
                .flex_row()
                .flex_grow(1.0)
                .width(SizeDimension::Percent(1.0)),
            &[spacer, content],
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
            spacer,
            topbar_node,
            topbar,
            sidebar_scroll,
            sidebar_scroll_node,
            sidebar_content_node,
            nav_host,
            menu_open,
            mobile: false,
            win_w: 0.0,
            win_h: 0.0,
            // Defaults to the content pane so a scroll before the first pointer event does the expected thing.
            ptr_x: f32::MAX,
            ptr_y: 0.0,
        })
    }

    /// The rail's left edge. The rail is painted as an overlay at a position this shell picks, not laid out
    /// in the body row, so mirroring it under RTL is the app's job — layout cannot move a hand-placed node.
    fn rail_x(&self) -> f32 {
        if use_direction().is_rtl() {
            self.win_w - SIDEBAR_W
        } else {
            0.0
        }
    }

    /// Dispatches into the rail through the same transform `view` paints it under, so a press lands where the
    /// user sees the button rather than 248px away once the rail has mirrored.
    fn sidebar_on_event(&mut self, event: &Event) -> EventResult {
        let rail_x = self.rail_x();
        if rail_x == 0.0 {
            return self.sidebar_scroll.on_event(event);
        }
        match transform_pointer(event, [1.0, 0.0, 0.0, 1.0, rail_x, 0.0]) {
            Some(local) => self.sidebar_scroll.on_event(&local),
            None => self.sidebar_scroll.on_event(event),
        }
    }

    /// Reconciles the content pane after the sidebar handled a press. The host only reconciles from events
    /// dispatched into it, and the rail sits outside its subtree — so a tab press has to be pushed in here.
    /// Only a press that actually moved the user closes the mobile drawer, leaving the theme switcher (also in
    /// the rail) free to keep it open.
    fn after_sidebar_press(&mut self) {
        if self.nav_host.sync() && self.mobile {
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
        // Pin the window-spanning root as the overlay host: the sidebar is computed as its own parent-less
        // root below, and auto-detection would otherwise make it (the 248px sidebar) the host — so
        // modals/drawers/menus would portal over the sidebar instead of the viewport.
        set_overlay_host(self.root);
        // The active page re-lays its own content: its scroll viewport now has the width the pass above gave it.
        self.nav_host.relayout();
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
    }
}

impl Component for ShellPage {
    fn view(&self) -> RenderNode {
        let open = self.menu_open.get();
        let mut nodes = vec![self.nav_host.view()];
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
            let rail = RenderNode::transform_with(
                [1.0, 0.0, 0.0, 1.0, self.rail_x(), 0.0],
                [
                    RenderNode::rect(
                        Rect {
                            x: 0.0,
                            y: 0.0,
                            width: SIDEBAR_W,
                            height: self.win_h,
                        },
                        RectStyle::default().with_fill(theme().surface_alt),
                    ),
                    self.sidebar_scroll.view(),
                ],
            );
            overlay.push(rail);
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
        let rail_x = self.rail_x();
        let over_sidebar = (rail_x..rail_x + SIDEBAR_W).contains(&self.ptr_x);

        if self.mobile {
            if self.menu_open.get() {
                if let Event::Scrolled { .. } = event {
                    // Only the drawer scrolls while it is open; a scroll over the scrim is swallowed.
                    if over_sidebar {
                        self.sidebar_on_event(event);
                    }
                    return EventResult::Handled;
                }
                // Drawer open: the sidebar (theme + nav buttons) hit-tests first; a press off the drawer closes it.
                if self.sidebar_on_event(event) == EventResult::Handled {
                    self.after_sidebar_press();
                    return EventResult::Handled;
                }
                if let Event::PointerPressed { x, .. } = event {
                    if !(rail_x..rail_x + SIDEBAR_W).contains(&(*x as f32)) {
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
            return self.nav_host.on_event(event);
        }
        // Desktop: scroll goes to whichever pane the pointer is over; the rail never eats the content's scroll.
        if let Event::Scrolled { .. } = event {
            return if over_sidebar {
                self.sidebar_on_event(event)
            } else {
                self.nav_host.on_event(event)
            };
        }
        // Other events: the sidebar rail hit-tests first (by coords), then the content pane.
        if self.sidebar_on_event(event) == EventResult::Handled {
            self.after_sidebar_press();
            return EventResult::Handled;
        }
        self.nav_host.on_event(event)
    }
}

pub struct SandboxRoot;

impl App for SandboxRoot {
    fn root(&self) -> Box<dyn telar::Component> {
        reset_layout_runtime();
        // Every section's stack is hot state, not just the active one: a reload lands you back where you were reading, in the section you were reading, with each section's own history intact.
        let sections: Vec<usize> = (0..SECTIONS.len()).collect();
        let stacks = TabStacks::new(
            hot_signal("sandbox::section", 0usize),
            &sections,
            |section| {
                Navigator::from_signal(
                    hot_signal(
                        &format!("sandbox::stack::{section}"),
                        vec![SectionRoute::Overview],
                    ),
                    SectionRoute::Overview,
                )
            },
        )
        // The rail is a table of contents, so Back means "what I was just reading": it walks out of a source listing first, then back through the sections visited to get here. Without this a section's Back is inert the moment its own stack is at its root, which for a docs shell reads as broken.
        .with_tab_history();
        let sidebar = build_sidebar(stacks.clone()).expect("sidebar build failed");
        let page = ShellPage::new(sidebar, stacks).expect("shell layout failed");
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(theme().background)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every entry's `.rsx` file must exist and carry a view — the macro table pairs a builder with a file name
    // by hand, so a renamed or moved feature would otherwise bake in the wrong source.
    #[test]
    fn every_section_bakes_the_source_of_its_own_feature() {
        for def in SECTIONS {
            assert!(def.file.ends_with(".rsx"), "{}: {}", def.title, def.file);
            assert!(
                def.source.contains("[view]"),
                "{} baked no view from {}",
                def.title,
                def.file
            );
        }
    }

    fn shell_stacks() -> (TabHost<usize, SectionRoute>, TabStacks<usize, SectionRoute>) {
        reset_layout_runtime();
        telar::set_theme(crate::core::theme::SandboxTheme::modern());
        let sections: Vec<usize> = (0..SECTIONS.len()).collect();
        let stacks = TabStacks::new(signal(5usize), &sections, |_| {
            Navigator::new(SectionRoute::Overview)
        });
        let factory = stacks.clone();
        let host = TabHost::new(
            stacks.clone(),
            move |section: &usize, route: &SectionRoute| build_page(&factory, *section, *route),
        )
        .unwrap()
        .with_policy(PagePolicy::Transient);
        (host, stacks)
    }

    // The source listing is a pushed page, not an overlay: navigating in deepens that section's own stack, Back returns to the overview, and a Back at the root reports "nothing to do" so a hardware back can fall through to the OS.
    #[test]
    fn source_detail_pushes_a_page_and_back_returns_to_the_section() {
        let (mut host, stacks) = shell_stacks();

        stacks.push(SectionRoute::Source);
        host.sync();
        assert_eq!(host.current_route(), Some(SectionRoute::Source));
        assert_eq!(stacks.depth(), 2);

        assert!(stacks.back());
        host.sync();
        assert_eq!(host.current_route(), Some(SectionRoute::Overview));
        assert!(
            !stacks.back(),
            "at a section's root with nothing open there is nothing to go back to"
        );
    }

    // The point of one stack per section: switching away from a section you had drilled into and coming back finds it still drilled in, without any keep-alive policy holding the page up by route.
    #[test]
    fn a_section_keeps_its_own_depth_while_you_read_another() {
        let (mut host, stacks) = shell_stacks();
        stacks.push(SectionRoute::Source);
        host.sync();

        stacks.select(9);
        host.sync();
        assert_eq!(host.current_tab(), 9);
        assert_eq!(
            host.current_route(),
            Some(SectionRoute::Overview),
            "arriving at a section lands on its overview, not the depth of the one you left"
        );

        stacks.select(5);
        host.sync();
        assert_eq!(
            host.current_route(),
            Some(SectionRoute::Source),
            "the section you left is still showing its source listing"
        );
    }

    // Pressing the rail item you are already reading is the way back out of its source listing, which is what keeps every item live instead of inert on the section you are on.
    #[test]
    fn reselecting_the_current_section_returns_to_its_overview() {
        let (mut host, stacks) = shell_stacks();
        stacks.push(SectionRoute::Source);
        host.sync();

        stacks.select(5);
        host.sync();
        assert_eq!(host.current_route(), Some(SectionRoute::Overview));
        assert_eq!(stacks.depth(), 1);
    }
}
