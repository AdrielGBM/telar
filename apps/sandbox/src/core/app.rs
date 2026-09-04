//! The documentation shell: the rail, the top bar, and the per-section navigation stacks behind them.

use crate::core::theme::theme;
use telar::{
    AlignItems, App, AvailableSpace, BorderRadius, Children, Color, Component, Container, Event,
    EventResult,
    JustifyContent, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, NavPage, NavTransition,
    Navigator, NodeId, NodeVec, PagePolicy, Rect, RectStyle, RenderNode, Role, RwSignal, ShapeStyle,
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

/// One doc section: its nav label, its content builder, and the `.rsx` file behind it (name plus the source text itself, baked in at compile time for the source detail page).
struct SectionDef {
    title: &'static str,
    build: SectionBuild,
    file: &'static str,
    source: &'static str,
}

/// A destination *within* a section. The section itself is not part of this: each rail item is a tab with its own stack, so the route only has to say how deep into that section you are — the overview a reader lands on, or the source listing pushed over it. Back returns to the overview at the scroll position it had.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
enum SectionRoute {
    Overview,
    Source,
}

/// Builds the [`SECTIONS`] table, baking each entry's `.rsx` source in beside its builder so a section stays a one-line edit. `include_str!` needs a literal path, which is why the file name is spelled out per entry.
macro_rules! sections {
    ($(($title:literal, $build:path, $props:ty, $file:literal)),* $(,)?) => {
        &[$(SectionDef {
            title: $title,
            // A section is a component that names no props and places no children, so it is called like one. The closure captures nothing, which is what lets it stand where a `fn` pointer is wanted.
            build: || $build(<$props>::props().build(), Children::default()),
            file: $file,
            source: include_str!(concat!("../features/", $file)),
        }),*]
    };
}

/// Every doc section in display order. The index is the section id, and this is the single source of truth the sidebar nav and the content pane both derive from: adding or reordering a section is a one-line edit here, not three in sync.
const SECTIONS: &[SectionDef] = sections![
    ("Overview", crate::features::overview::overview, crate::features::overview::OverviewProps, "overview.rsx"),
    ("Layout", crate::features::layout::layout, crate::features::layout::LayoutProps, "layout.rsx"),
    ("Sizing & grid", crate::features::sizing::sizing, crate::features::sizing::SizingProps, "sizing.rsx"),
    ("Typography", crate::features::typography::typography, crate::features::typography::TypographyProps, "typography.rsx"),
    ("Color & theme", crate::features::color::color, crate::features::color::ColorProps, "color.rsx"),
    ("Boxes & borders", crate::features::boxes::boxes, crate::features::boxes::BoxesProps, "boxes.rsx"),
    ("Gradients", crate::features::gradients::gradients, crate::features::gradients::GradientsProps, "gradients.rsx"),
    ("Shadows", crate::features::shadows::shadows, crate::features::shadows::ShadowsProps, "shadows.rsx"),
    ("Opacity & layers", crate::features::opacity::opacity, crate::features::opacity::OpacityProps, "opacity.rsx"),
    ("Images", crate::features::images::images, crate::features::images::ImagesProps, "images.rsx"),
    ("SVG", crate::features::svg::svg, crate::features::svg::SvgProps, "svg.rsx"),
    ("Paths", crate::features::paths::paths, crate::features::paths::PathsProps, "paths.rsx"),
    ("Transforms", crate::features::transforms::transforms, crate::features::transforms::TransformsProps, "transforms.rsx"),
    ("Buttons", crate::features::buttons::buttons, crate::features::buttons::ButtonsProps, "buttons.rsx"),
    ("Form controls", crate::features::forms::forms, crate::features::forms::FormsProps, "forms.rsx"),
    ("Sliders", crate::features::sliders::sliders, crate::features::sliders::SlidersProps, "sliders.rsx"),
    (
        "Text fields",
        crate::features::text_fields::text_fields,
        crate::features::text_fields::TextFieldsProps,
        "text_fields.rsx"
    ),
    ("Stepper", crate::features::steppers::steppers, crate::features::steppers::SteppersProps, "steppers.rsx"),
    (
        "Progress & spinner",
        crate::features::indicators::indicators,
        crate::features::indicators::IndicatorsProps,
        "indicators.rsx"
    ),
    (
        "Tabs & accordion",
        crate::features::navigation::navigation,
        crate::features::navigation::NavigationProps,
        "navigation.rsx"
    ),
    ("Badges & chips", crate::features::pills::pills, crate::features::pills::PillsProps, "pills.rsx"),
    ("Menus & select", crate::features::menus::menus, crate::features::menus::MenusProps, "menus.rsx"),
    ("Dialogs & overlays", crate::features::dialogs::dialogs, crate::features::dialogs::DialogsProps, "dialogs.rsx"),
    ("Reactivity", crate::features::reactivity::reactivity, crate::features::reactivity::ReactivityProps, "reactivity.rsx"),
    (
        "Transitions",
        crate::features::transitions::transitions,
        crate::features::transitions::TransitionsProps,
        "transitions.rsx"
    ),
    ("Motion", crate::features::motion::motion, crate::features::motion::MotionProps, "motion.rsx"),
    (
        "Background work",
        crate::features::background::background,
        crate::features::background::BackgroundProps,
        "background.rsx"
    ),
    ("Positioning", crate::features::positioning::positioning, crate::features::positioning::PositioningProps, "positioning.rsx"),
    ("Pointer & drag", crate::features::pointer::pointer, crate::features::pointer::PointerProps, "pointer.rsx"),
];

/// A restored hot-reload stack (or a deep link) can name a section that no longer exists; clamp rather than panic.
fn section_def(section: usize) -> &'static SectionDef {
    &SECTIONS[section.min(SECTIONS.len() - 1)]
}

/// One doc section as a navigable page: the section's content in a reading column, inside its own scroll viewport. Each page scrolling itself is what lets a section keep its reading position while another is on screen — navigating back returns to where you were, as a page stack should.
struct SectionPage {
    scroll: LayoutScrollArea,
}

impl SectionPage {
    fn new(nav: Navigator<SectionRoute>, section: usize) -> Result<Self, LayoutError> {
        let def = section_def(section);
        let column = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .max_width(960.0)
                .padding_all(32.0)
                .gap(40.0),
            vec![(def.build)()?, build_source_link(nav, section)?],
        )?
        // The one region this screen is *for*. What a reader jumps to first and a document writes as `<main>`.
        .role(Role::Main);
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

/// Footer of a section page: pushes that section's `.rsx` source as a detail page. A push rather than an overlay because the listing is long — its scroll position is state the stack should remember, and Back is the way out.
fn build_source_link(
    nav: Navigator<SectionRoute>,
    section: usize,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let file = section_def(section).file;
    let label = Text::new(
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
    .hover_style(|_r| {
        RectStyle::default()
            .with_fill(theme().border)
            .with_radius(BorderRadius::all(8.0))
    })
    .on_press(move || nav.push(SectionRoute::Source));
    Ok(Box::new(Container::new(
        LayoutStyle::new().flex_row(),
        vec![Box::new(btn)],
    )?))
}

/// The `.rsx` source behind a section, as a pushed detail page: the file name above the listing, in its own scroll viewport.
///
/// The listing is one wrapped text block rather than a [`LineGutter`](telar::LineGutter) column: a gutter numbers *logical* lines, and with no monospace or no-wrap in `TextStyle` a long line soft-wraps, which would slide the numbers out of step with the code.
struct SourcePage {
    scroll: LayoutScrollArea,
}

impl SourcePage {
    fn new(section: usize) -> Result<Self, LayoutError> {
        let def = section_def(section);
        let (file, source) = (def.file, def.source);
        let heading = Text::new(
            move || file.to_string(),
            LayoutStyle::new(),
            || TextStyle::new(20.0, theme().ink).with_font_weight(700),
        )?;
        let listing = Text::new(
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

/// Builds whichever page a route names inside `section` — the [`TabHost`]'s factory, called on a route's first visit to that section's own stack. The stack handed to the overview is that section's, so its source link pushes onto the section it belongs to rather than onto whatever tab happens to be active.
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

/// Base paint for a nav item: the active section is filled with the accent; the rest blend into the rail (its `surface_alt` panel), so an inactive item reads as flat until hovered.
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

/// Back control: closes an open dialog if there is one, else pops the current section's own stack (out of a source listing, back to its overview), else returns to the section read before this one. Always present, but it dims to `muted` once there is nothing left to go back to, so it reads as unavailable without the layout shifting when history appears.
fn build_back(stacks: TabStacks<usize, SectionRoute>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // Live when there is either a dialog to close or a page to pop; both reads are reactive, so the control lights up the moment a modal opens even at the root of a section's stack.
    let live =
        move |stacks: &TabStacks<usize, SectionRoute>| use_dismiss_depth() > 0 || stacks.can_pop();
    let on_label = stacks.clone();
    let label = Text::new(
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
    .hover_style(move |_r| {
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

/// Contents nav: one full-width button per section, above a back control. The active one is highlighted. Each item is a *tab*, not a history entry: selecting one switches to that section's own stack, leaving the one you came from standing exactly where it was — a source listing still open, scrolled where you left it. Pressing the section you are already reading pops it back to its overview, so an item is never inert.
fn build_nav(stacks: TabStacks<usize, SectionRoute>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut buttons: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(SECTIONS.len());
    for (i, def) in SECTIONS.iter().enumerate() {
        let title = def.title;
        let on_label = stacks.clone();
        let label = Text::new(
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
        let btn = StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER)
                .padding_horizontal(14.0)
                .padding_vertical(3.0),
            move |_r| nav_rect(on_base.active() == i),
            vec![Box::new(label)],
        )?
        .hover_style(move |_r| nav_rect_hover(on_hover.active() == i))
        .on_press(move || on_press.select(i));
        // A `list` is a role with a content model: every child of one has to be a `listitem`, exactly as under a `<ul>`. The button is the control inside the item, not the item — without the wrapper the rail is a list of twenty-four things a reader is told are not list items, and the count it would have announced goes with them.
        buttons.push(Box::new(
            Container::new(LayoutStyle::new().flex_column(), vec![Box::new(btn)])?
                .role(Role::ListItem),
        ));
    }
    let list = Container::new(LayoutStyle::new().flex_column().gap(3.0), buttons)?.role(Role::List);
    let label = Text::single_line(
        || "CONTENTS".to_string(),
        || TextStyle::new(11.0, theme().muted),
    )?;
    Ok(Box::new(
        Container::new(
            LayoutStyle::new().flex_column().gap(8.0),
            vec![build_back(stacks)?, Box::new(label), Box::new(list)],
        )?
        .role(Role::Navigation),
    ))
}

/// Full sidebar: the `.rsx` header + theme switcher, then the Rust-built section nav.
fn build_sidebar(
    stacks: TabStacks<usize, SectionRoute>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let header_theme = crate::core::sidebar::sidebar(crate::core::sidebar::SidebarProps::props().build(), Children::default())?;
    let nav = build_nav(stacks)?;
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_column()
            .width(SIDEBAR_W)
            .padding_all(20.0)
            .gap(22.0),
        vec![header_theme, nav],
    )?
    // Supporting content beside the section being read, which is what a rail of links to other sections is. A reader can jump straight past it; a document writes it as an `<aside>`.
    .role(Role::Complementary)))
}

/// Mobile top bar: a hamburger button (toggles `menu_open`) next to the wordmark. Shown only below the breakpoint.
fn build_topbar(menu_open: RwSignal<bool>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let toggle = menu_open;
    let glyph = Text::new(
        || "\u{2630}".to_string(),
        LayoutStyle::new(),
        || TextStyle::new(20.0, theme().ink),
    )?;
    let burger = StyledContainer::new(
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
    .hover_style(|_r| {
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

/// Responsive application shell. Desktop (≥ breakpoint): the sidebar is a fixed rail whose width is reserved by an empty `spacer` and painted as an always-on overlay at the window's left edge. Mobile (< breakpoint): the rail collapses — a top bar appears, and the sidebar becomes a drawer that slides over a dimming scrim when `menu_open` is set. The sidebar is laid out on its own so it can overlay content in either mode.
///
/// The content pane is a [`TabHost`]: every rail item is a tab with its own page stack, built the first time it is visited, so opening the app costs one section rather than all 26.
struct ShellPage {
    root: NodeId,
    spacer: NodeId,
    topbar_node: NodeId,
    topbar: Box<dyn LayoutItem>,
    sidebar_scroll: LayoutScrollArea,
    sidebar_scroll_node: NodeId,
    sidebar_content_node: NodeId,
    nav_host: TabHost<usize, SectionRoute>,
    menu_open: RwSignal<bool>,
    mobile: bool,
    win_w: f32,
    win_h: f32,
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
        // Every page is a stack entry: a section survives being left because its stack stays alive, not because the host pins it by route. So a source listing is fresh on each push and two visits never share a scroll.
        .with_policy(PagePolicy::Transient)
        .with_transition(NavTransition::Fade)
        .with_tab_transition(NavTransition::Fade);
        let sidebar_scroll = LayoutScrollArea::new(
            LayoutStyle::new()
                .width(SIDEBAR_W)
                .height(SizeDimension::Percent(1.0)),
            sidebar,
        )?;
        let sidebar_scroll_node = sidebar_scroll.layout_node();
        let menu_open = signal(false);
        let topbar = build_topbar(menu_open)?;
        let topbar_node = topbar.layout_node();
        let (spacer, _) = new_leaf(
            LayoutStyle::new()
                .width(SIDEBAR_W)
                .height(SizeDimension::Percent(1.0)),
        )?;
        // The host's own container is `width: 100%`, which as a flex-basis beside the 248px spacer would overflow the row; this wrapper gives it the remaining width to be 100% of instead.
        let content = new_container(
            LayoutStyle::new().flex_row().flex_grow(1.0),
            &[nav_host.layout_node()],
        )?;
        let body = new_container(
            LayoutStyle::new()
                .flex_row()
                .flex_grow(1.0)
                .width(SizeDimension::Percent(1.0)),
            &[spacer, content],
        )?;
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
        })
    }

    /// The rail's left edge. The rail is painted as an overlay at a position this shell picks, not laid out in the body row, so mirroring it under RTL is the app's job — layout cannot move a hand-placed node.
    fn rail_x(&self) -> f32 {
        if use_direction().is_rtl() {
            self.win_w - SIDEBAR_W
        } else {
            0.0
        }
    }

    /// Dispatches into the rail through the same transform `view` paints it under, so a press lands where the user sees the button rather than 248px away once the rail has mirrored.
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

    /// Reconciles the content pane after the sidebar handled a press. The host only reconciles from events dispatched into it, and the rail sits outside its subtree — so a tab press has to be pushed in here. Only a press that actually moved the user closes the mobile drawer, leaving the theme switcher (also in the rail) free to keep it open.
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
        if !mobile {
            self.menu_open.set(false);
        }
        set_display(self.topbar_node, mobile);
        set_display(self.spacer, !mobile);
        mark_dirty(self.root).ok();
        compute_layout(
            self.root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(height),
        )
        .ok();
        // Pin the window-spanning root as the overlay host: the sidebar is computed as its own parent-less root below, and auto-detection would otherwise make that 248px sidebar the host, portaling modals over it.
        set_overlay_host(self.root);
        self.nav_host.relayout();
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
            // Besides bounding the overlay to the window, the clip is a structural boundary that stops the hardware batcher reordering the top bar's text above the drawer background.
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
        let rail_x = self.rail_x();
        let over_sidebar = match event {
            Event::PointerMoved { x, .. }
            | Event::PointerPressed { x, .. }
            | Event::PointerReleased { x, .. }
            | Event::Scrolled { x, .. } => (rail_x..rail_x + SIDEBAR_W).contains(&(*x as f32)),
            _ => false,
        };

        if self.mobile {
            if self.menu_open.get() {
                if let Event::Scrolled { .. } = event {
                    if over_sidebar {
                        self.sidebar_on_event(event);
                    }
                    return EventResult::Handled;
                }
                if self.sidebar_on_event(event) == EventResult::Handled {
                    self.after_sidebar_press();
                    return EventResult::Handled;
                }
                if let Event::PointerPressed { x, .. } = event {
                    if !(rail_x..rail_x + SIDEBAR_W).contains(&(*x as f32)) {
                        self.menu_open.set(false);
                    }
                }
                return EventResult::Handled;
            }
            if self.topbar.on_event(event) == EventResult::Handled {
                return EventResult::Handled;
            }
            return self.nav_host.on_event(event);
        }
        if let Event::Scrolled { .. } = event {
            return if over_sidebar {
                self.sidebar_on_event(event)
            } else {
                self.nav_host.on_event(event)
            };
        }
        if self.sidebar_on_event(event) == EventResult::Handled {
            self.after_sidebar_press();
            return EventResult::Handled;
        }
        self.nav_host.on_event(event)
    }
}

/// The documentation shell's root component.
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
        // The rail is a table of contents, so Back means "what I was just reading". Without this a section's Back is inert the moment its own stack is at its root, which for a docs shell reads as broken.
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

    // Every entry's `.rsx` file must exist and carry a view — the macro table pairs a builder with a file name by hand, so a renamed or moved feature would otherwise bake in the wrong source.
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
