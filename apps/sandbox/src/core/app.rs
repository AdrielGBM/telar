use crate::core::theme::theme;
use rsx::{
    AlignItems, App, AvailableSpace, BorderRadius, Color, Component, Container, Event, EventResult,
    JustifyContent, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, NavHost, NavPage,
    NavTransition, Navigator, NodeId, NodeVec, Rect, RectStyle, RenderNode, RwSignal, ShapeStyle,
    PagePolicy, SizeDimension, StyledContainer, Text, TextStyle, compute_layout, hot_signal, mark_dirty, use_dismiss_depth, new_container,
    new_leaf, reset_layout_runtime, set_display, set_overlay_host, signal,
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

/// A navigation destination. A section is the overview a reader lands on; its source is the detail pushed from
/// it — so navigating in adds a stack entry and Back returns to the section, at the scroll position it had.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
enum Route {
    Section(usize),
    Source(usize),
}

impl Route {
    /// The section this destination belongs to, so the rail highlights it from the source page too.
    fn section(self) -> usize {
        match self {
            Route::Section(i) | Route::Source(i) => i,
        }
    }
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
    ("Text fields", crate::features_text_fields, "text_fields.rsx"),
    ("Stepper", crate::features_steppers, "steppers.rsx"),
    ("Progress & spinner", crate::features_indicators, "indicators.rsx"),
    ("Tabs & accordion", crate::features_navigation, "navigation.rsx"),
    ("Badges & chips", crate::features_pills, "pills.rsx"),
    ("Menus & select", crate::features_menus, "menus.rsx"),
    ("Dialogs & overlays", crate::features_dialogs, "dialogs.rsx"),
    ("Reactivity", crate::features_reactivity, "reactivity.rsx"),
    ("Transitions", crate::features_transitions, "transitions.rsx"),
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
    fn new(nav: Navigator<Route>, section: usize) -> Result<Self, LayoutError> {
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
fn build_source_link(nav: Navigator<Route>, section: usize) -> Result<Box<dyn LayoutItem>, LayoutError> {
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
    .on_press(move || nav.push(Route::Source(section)));
    // A row so the button hugs its label instead of stretching across the reading column.
    Ok(Box::new(Container::new(
        LayoutStyle::new().flex_row(),
        vec![Box::new(btn)],
    )?))
}

/// The `.rsx` source behind a section, as a pushed detail page: the file name above the listing, in its own
/// scroll viewport.
///
/// The listing is one wrapped text block rather than a [`LineGutter`](rsx::LineGutter) column: a gutter numbers
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

/// Builds whichever page a route names — the [`NavHost`]'s factory, called on a route's first visit.
fn build_page(nav: &Navigator<Route>, route: Route) -> Result<Box<dyn NavPage>, LayoutError> {
    Ok(match route {
        Route::Section(i) => Box::new(SectionPage::new(nav.clone(), i)?) as Box<dyn NavPage>,
        Route::Source(i) => Box::new(SourcePage::new(i)?),
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

/// Back control: closes an open dialog if there is one, else returns to the previously viewed section. Always
/// present and always live — `back` is a no-op with nothing open at the root — but it dims to `muted` there so
/// it reads as unavailable without the layout shifting when history appears.
fn build_back(nav: Navigator<Route>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // Live when there is either a dialog to close or a page to pop; both reads are reactive, so the control
    // lights up the moment a modal opens even at the root of the stack.
    let live = move |nav: &Navigator<Route>| use_dismiss_depth() > 0 || nav.can_pop();
    let on_label = nav.clone();
    let label = Text::auto(
        || "\u{2190} Back".to_string(),
        LayoutStyle::new(),
        move || {
            let t = theme();
            let color = if live(&on_label) { t.ink } else { t.muted };
            TextStyle::new(13.0, color)
        },
    )?;
    let on_hover = nav.clone();
    let on_press = nav.clone();
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

/// Contents nav: one full-width button per section that navigates to it, above a back control. The active
/// one is highlighted. Selecting a section *pushes* it, so the stack doubles as reading history — Back walks
/// the sections you came through, the way a browser does. A section stays highlighted while its source detail
/// is on screen, since that page is part of the section rather than a sibling of it.
fn build_nav(nav: Navigator<Route>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut buttons: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(SECTIONS.len());
    for (i, def) in SECTIONS.iter().enumerate() {
        let title = def.title;
        let on_label = nav.clone();
        let label = Text::auto(
            move || title.to_string(),
            LayoutStyle::new(),
            move || {
                let t = theme();
                let color = if on_label.current().section() == i {
                    t.on_primary
                } else {
                    t.ink
                };
                TextStyle::new(13.0, color)
            },
        )?;
        let on_base = nav.clone();
        let on_hover = nav.clone();
        let on_press = nav.clone();
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
            move |_r| nav_rect(on_base.current().section() == i),
            vec![Box::new(label)],
        )?
        .on_hover_style(move |_r| nav_rect_hover(on_hover.current().section() == i))
        // Pressing the section a source page belongs to navigates up to the section itself, so the rail is
        // never inert; only re-pressing the section you are already reading does nothing.
        .on_press(move || {
            if on_press.peek_current() != Route::Section(i) {
                on_press.push(Route::Section(i));
            }
        });
        buttons.push(Box::new(btn));
    }
    let list = Container::new(LayoutStyle::new().flex_column().gap(3.0), buttons)?;
    let label = Text::single_line(
        || "CONTENTS".to_string(),
        || TextStyle::new(11.0, theme().muted),
    )?;
    Ok(Box::new(Container::new(
        LayoutStyle::new().flex_column().gap(8.0),
        vec![build_back(nav)?, Box::new(label), Box::new(list)],
    )?))
}

/// Full sidebar: the `.rsx` header + theme switcher, then the Rust-built section nav.
fn build_sidebar(nav: Navigator<Route>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let header_theme = crate::core_sidebar()?;
    let nav = build_nav(nav)?;
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
/// The content pane is a [`NavHost`]: it builds a section the first time it is navigated to and caches it,
/// so opening the app costs one section rather than all 26.
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
    nav_host: NavHost<Route>,
    menu_open: RwSignal<bool>,
    mobile: bool,
    win_w: f32,
    win_h: f32,
    // Last pointer position, tracked so coordinate-less Scrolled events route to whichever pane the pointer is over.
    ptr_x: f32,
    ptr_y: f32,
}

impl ShellPage {
    fn new(sidebar: Box<dyn LayoutItem>, nav: Navigator<Route>) -> Result<Self, LayoutError> {
        let sidebar_content_node = sidebar.layout_node();
        let factory_nav = nav.clone();
        let nav_host = NavHost::new(nav.clone(), move |route: &Route| {
            build_page(&factory_nav, *route)
        })?
        // The rail's sections are persistent destinations — coming back to one shows it as you left it,
        // reading position included. A source listing is a pushed screen: fresh on each push, released when
        // you go back, so two visits to the same file never share one scroll position.
        .with_policy_for(|route: &Route| match route {
            Route::Section(_) => PagePolicy::KeepAlive,
            Route::Source(_) => PagePolicy::Transient,
        })
        .with_transition(NavTransition::Fade);
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

    /// Reconciles the content pane after the sidebar handled a press. The host only reconciles from events
    /// dispatched into it, and the rail sits outside its subtree — so a nav press has to be pushed in here.
    /// Only a press that actually navigated closes the mobile drawer, leaving the theme switcher (also in the
    /// rail) free to keep it open.
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
                    self.after_sidebar_press();
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
            return self.nav_host.on_event(event);
        }
        // Desktop: scroll goes to whichever pane the pointer is over; the rail never eats the content's scroll.
        if let Event::Scrolled { .. } = event {
            return if over_sidebar {
                self.sidebar_scroll.on_event(event)
            } else {
                self.nav_host.on_event(event)
            };
        }
        // Other events: the sidebar rail hit-tests first (by coords), then the content pane.
        if self.sidebar_scroll.on_event(event) == EventResult::Handled {
            self.after_sidebar_press();
            return EventResult::Handled;
        }
        self.nav_host.on_event(event)
    }
}

pub struct SandboxRoot;

impl App for SandboxRoot {
    fn root(&self) -> Box<dyn rsx::Component> {
        reset_layout_runtime();
        // The stack — not just the current section — is hot state: a reload lands you back where you were
        // reading, with your history intact.
        let root = Route::Section(0);
        let nav = Navigator::from_signal(hot_signal("sandbox::nav_stack", vec![root]), root);
        let sidebar = build_sidebar(nav.clone()).expect("sidebar build failed");
        let page = ShellPage::new(sidebar, nav).expect("shell layout failed");
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

    // The source listing is a pushed page, not an overlay: navigating in deepens the stack, Back returns to the
    // section, and a Back at the root reports "nothing to do" so a hardware back can fall through to the OS.
    #[test]
    fn source_detail_pushes_a_page_and_back_returns_to_the_section() {
        reset_layout_runtime();
        rsx::set_theme(crate::core::theme::SandboxTheme::modern());
        let nav = Navigator::new(Route::Section(5));
        let factory_nav = nav.clone();
        let mut host =
            NavHost::new(nav.clone(), move |r: &Route| build_page(&factory_nav, *r)).unwrap();

        nav.push(Route::Source(5));
        host.sync();
        assert_eq!(host.current(), Route::Source(5));
        assert_eq!(nav.depth(), 2);

        assert!(nav.back());
        host.sync();
        assert_eq!(host.current(), Route::Section(5));
        assert!(!nav.back(), "at the root with nothing open there is nothing to go back to");
    }

    // The rail highlights by section, so a source page keeps its section lit rather than clearing the rail.
    #[test]
    fn a_source_route_belongs_to_its_section() {
        assert_eq!(Route::Source(7).section(), Route::Section(7).section());
    }
}
