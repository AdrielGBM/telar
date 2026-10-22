//! Resolution is lazy and keyed by signals per declaration, so a walk subscribes only to the ancestors that actually declare, and changing one declaration re-renders only the leaves beneath it.

use std::rc::Rc;

use layout_core::{Direction, NodeId};
use platform_core::Cursor;
use reactive_core::{OwnerId, RwSignal, signal};
use renderer_core::{Declared, TextStyle};
use rustc_hash::FxHashMap;
use theme_core::{ScopedTheme, ThemeTokens, use_theme_tokens};

/// `text` carries only the inherited half of a text style; `max_lines`/`ellipsis` are reset-only and stay wherever [`Inherited::initial`] put them.
#[derive(Clone, PartialEq, Debug)]
pub struct Inherited {
    pub text: TextStyle,
    pub cursor: Cursor,
    pub direction: Direction,
}

impl Inherited {
    /// `telar/src/text_style_baseline_test.rs` asserts these values against a real frame, so changing one is a decision.
    pub fn initial() -> Self {
        Self::from_tokens(&*use_theme_tokens())
    }

    /// Text size inherits, so this stays a constant rather than a theme token — a theme instead sets [`ThemeTokens::root`].
    pub const BASE_FONT_SIZE: f32 = 14.0;

    /// The row `tokens` set, over the active surface's default family ([`set_font_family`](crate::set_font_family)) — read reactively, so a cascade resolved from here follows it.
    pub fn from_tokens(tokens: &dyn ThemeTokens) -> Self {
        let base = TextStyle::new(Self::BASE_FONT_SIZE, tokens.ink())
            .with_font_family(crate::surface_font::root_family());
        Self {
            text: {
                let root = tokens.root();
                root.over(&base, surface_for(&root))
            },
            cursor: Cursor::Default,
            direction: Direction::Ltr,
        }
    }

    pub fn with(&self, declared: &Declared) -> Self {
        Self {
            text: declared.over(&self.text, surface_for(declared)),
            ..self.clone()
        }
    }

    pub fn text_style(&self) -> TextStyle {
        self.text.clone()
    }
}

/// The surface a declaration resolves against: read reactively only by one that names a fraction of it, so a resize re-resolves exactly the text that depends on it.
fn surface_for(declared: &Declared) -> geometry_core::Size {
    if declared.uses_surface() {
        crate::context::use_surface_size()
    } else {
        geometry_core::Size::ZERO
    }
}

impl Default for Inherited {
    fn default() -> Self {
        Self::initial()
    }
}

reactive_core::surface_local! {
    /// Per surface: a texture UI and its window are different documents, so declarations must not cross between them.
    slot CASCADE: Cascade = Cascade::default();
    access with_cascade, with_cascade_ref;
    context CascadeContext, CascadeGuard;
}

struct Cascade {
    /// Behind a signal so reading it subscribes the reader; empty until something declares, so an undeclared walk costs one map miss per ancestor.
    declared: FxHashMap<NodeId, RwSignal<Declared>>,
    /// What the call placing a node said about it, apart from what the node's widget declared, because it lands on the other side of the node's own style: see [`author`].
    authored: FxHashMap<NodeId, RwSignal<Declared>>,
    /// Bumped only when nodes start or stop declaring, not on value changes, so a value write reruns only its own readers.
    structure: RwSignal<u64>,
    /// The nodes a [`provide_theme`](crate::provide_theme) put a theme of their own on. A walk stops at the nearest one and starts from that theme's root row, so a themed card inside a bar is set the way it would be in a window of its own, rather than in whatever the bar declared for its own text.
    themed: FxHashMap<NodeId, ThemeBoundary>,
    /// The root rows recently resolved, kept so nodes that resolve to the same value share it rather than each holding a copy. Compared by value because a theme swap and a light/dark flip both change a row, and neither is something the cascade can be told about.
    roots: Vec<Rc<Inherited>>,
}

struct ThemeBoundary {
    theme: ScopedTheme,
    /// The provider that put it here, so only that one takes it away.
    by: Option<OwnerId>,
}

/// How many distinct root rows [`Cascade::roots`] keeps: one per theme in force on a surface, and a surface carries a handful at most.
const ROOTS_KEPT: usize = 8;

impl Default for Cascade {
    fn default() -> Self {
        Self {
            declared: FxHashMap::default(),
            authored: FxHashMap::default(),
            structure: signal(0),
            themed: FxHashMap::default(),
            roots: Vec::new(),
        }
    }
}

/// Forgets every declaration, for a tree being replaced wholesale. See [`reset_layout_runtime`](crate::context::reset_layout_runtime) for why it cannot be done separately.
pub(crate) fn reset_cascade() {
    // In the surface's world because its signals outlive every scope: attributing them to whatever owner is resetting would free them with it.
    with_cascade(|c| *c = reactive_core::in_surface_world(Cascade::default));
}

/// `node` and every node above it, nearest first, crossing from a scroll's content to the area that shows it as an `overflow: auto` box's content inherits in CSS, though the layout tree keeps the two apart.
fn lineage(node: NodeId) -> Lineage {
    Lineage {
        climb: layout_reactive::ancestors(node),
        last: None,
        crossed: 0,
    }
}

/// How many scroll areas a climb crosses at most, which only a cycle the registry should never hold could reach.
const MAX_CROSSINGS: u32 = 64;

struct Lineage {
    climb: layout_reactive::Ancestors,
    last: Option<NodeId>,
    crossed: u32,
}

impl Iterator for Lineage {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        if let Some(node) = self.climb.next() {
            self.last = Some(node);
            return Some(node);
        }
        let area = layout_reactive::scroll_host(self.last.take()?)?;
        self.crossed += 1;
        if self.crossed > MAX_CROSSINGS {
            return None;
        }
        self.climb = layout_reactive::ancestors(area);
        self.next()
    }
}

/// Who said a declaration: the widget that built the node, or the call that placed it.
#[derive(Clone, Copy)]
enum Layer {
    Declared,
    Authored,
}

impl Cascade {
    fn layer(&self, layer: Layer) -> &FxHashMap<NodeId, RwSignal<Declared>> {
        match layer {
            Layer::Declared => &self.declared,
            Layer::Authored => &self.authored,
        }
    }

    fn layer_mut(&mut self, layer: Layer) -> &mut FxHashMap<NodeId, RwSignal<Declared>> {
        match layer {
            Layer::Declared => &mut self.declared,
            Layer::Authored => &mut self.authored,
        }
    }
}

/// Records what `node` says for everything beneath it.
pub fn declare(node: NodeId, declared: Declared) {
    record(Layer::Declared, node, declared);
}

/// Forgets what `node` declared, for a node leaving the tree or stopping.
pub fn undeclare(node: NodeId) {
    withdraw(Layer::Declared, node);
}

/// Records what the call that placed `node` says about its text, over whatever the widget behind it says: a text node takes it after its own style, and everything beneath inherits it as it would a declaration.
pub fn author(node: NodeId, declared: Declared) {
    record(Layer::Authored, node, declared);
}

/// What the call placing a widget says about its text, over what the widget itself says: `heading font_weight:normal` keeps the heading's role and size and takes the weight it was given, where a container's declaration around it would lose to the heading's own.
pub trait AuthoredText: crate::LayoutItem + Sized {
    /// Re-read when what `declared` reads changes, and withdrawn with the owner that said it.
    fn authored_text(self, declared: impl Fn() -> Declared + 'static) -> Self {
        let node = self.layout_node();
        reactive_core::effect(move || author(node, declared()));
        self
    }
}

impl<T: crate::LayoutItem> AuthoredText for T {}

fn record(layer: Layer, node: NodeId, declared: Declared) {
    let existing = with_cascade_ref(|c| c.layer(layer).get(&node).cloned());
    match (existing, declared.is_empty()) {
        (Some(sig), false) => {
            if sig.peek() != declared {
                sig.set(declared);
            }
        }
        (Some(_), true) => withdraw(layer, node),
        (None, false) => {
            let structure = with_cascade(|c| {
                c.layer_mut(layer).insert(node, signal(declared));
                c.structure
            });
            // The owner that started declaring is what stops: tying withdrawal to the widget's `Drop` instead raced the path that frees layout nodes.
            reactive_core::on_cleanup(move || withdraw(layer, node));
            structure.set(structure.peek().wrapping_add(1));
        }
        (None, true) => {}
    }
}

fn withdraw(layer: Layer, node: NodeId) {
    let structure = with_cascade(|c| c.layer_mut(layer).remove(&node).map(|_| c.structure));
    if let Some(structure) = structure {
        structure.set(structure.peek().wrapping_add(1));
    }
}

/// `own`, the style the text at `node` settled on, with what the call that placed it authored applied over it. A size in `em` there is of `em_base`, the size the text was handed, as an author's `font_size:1.5em` on a heading means half again the text around it rather than of the heading's own.
pub(crate) fn authored_over(
    node: NodeId,
    own: TextStyle,
    em_base: impl FnOnce() -> f32,
) -> TextStyle {
    let (structure, authored) = with_cascade_ref(|c| (c.structure, c.authored.get(&node).copied()));
    let _ = structure.get();
    let Some(authored) = authored else {
        return own;
    };
    let authored = authored.get();
    let surface = surface_for(&authored);
    authored
        .sized_against(em_base(), surface)
        .over(&own, surface)
}

/// Re-resolves every walk, for a subtree hung from a different parent: what it inherits is now what its new ancestors declare, and nothing about any one declaration changed to say so.
pub(crate) fn restructured() {
    let structure = with_cascade_ref(|c| c.structure);
    structure.set(structure.peek().wrapping_add(1));
}

/// Puts `theme` at the top of everything beneath `node`, for as long as the current owner lives. The first provider to claim a node keeps it, so nested providers give the shared node to the nearest one.
pub(crate) fn bind_theme(node: NodeId, theme: ScopedTheme) {
    let by = reactive_core::current_owner();
    let structure = with_cascade(|c| {
        if c.themed.contains_key(&node) {
            return None;
        }
        c.themed.insert(node, ThemeBoundary { theme, by });
        Some(c.structure)
    });
    let Some(structure) = structure else {
        return;
    };
    reactive_core::on_cleanup(move || unbind_theme(node, by));
    structure.set(structure.peek().wrapping_add(1));
}

fn unbind_theme(node: NodeId, by: Option<OwnerId>) {
    let structure = with_cascade(|c| {
        let bound_here = c
            .themed
            .get(&node)
            .is_some_and(|boundary| boundary.by == by);
        bound_here.then(|| {
            c.themed.remove(&node);
            c.structure
        })
    });
    if let Some(structure) = structure {
        structure.set(structure.peek().wrapping_add(1));
    }
}

/// The context in force at `node`: everything its ancestors declared, merged in tree order over the root row of the theme in force there. With no themed node above, the root row is the ambient theme's.
pub fn context(node: NodeId) -> Rc<Inherited> {
    let structure = with_cascade_ref(|c| c.structure);
    // A node that declares nothing has no signal to read, and it is exactly that node that may start.
    let _ = structure.get();

    let mut chain: Vec<Declared> = Vec::new();
    let mut theme = None;
    for current in lineage(node) {
        let (declared, authored, themed) = with_cascade_ref(|c| {
            (
                c.declared.get(&current).copied(),
                c.authored.get(&current).copied(),
                c.themed.get(&current).map(|boundary| boundary.theme),
            )
        });
        // Ahead of the node's own declaration, so it is applied after it and wins. A node's own authoring is left out: it lands after the node's own style, in `authored_over`, not before it.
        if current != node
            && let Some(sig) = authored
        {
            chain.push(sig.get());
        }
        if let Some(sig) = declared {
            chain.push(sig.get());
        }
        if themed.is_some() {
            theme = themed;
            break;
        }
    }
    let root = match theme {
        Some(theme) => root(Inherited::from_tokens(&*theme.tokens())),
        None => root(Inherited::initial()),
    };
    if chain.is_empty() {
        return root;
    }
    // Nearest last, so it is applied last and wins.
    let mut resolved = (*root).clone();
    for declared in chain.into_iter().rev() {
        resolved = resolved.with(&declared);
    }
    Rc::new(resolved)
}

/// `resolved` as a row shared with every other node that resolved the same one. Resolved on read, like the rest of the cascade, so a theme or mode switch repaints exactly the text that took a colour from it.
fn root(resolved: Inherited) -> Rc<Inherited> {
    with_cascade(|c| {
        if let Some(row) = c.roots.iter().find(|row| ***row == resolved) {
            return Rc::clone(row);
        }
        if c.roots.len() == ROOTS_KEPT {
            c.roots.remove(0);
        }
        let row = Rc::new(resolved);
        c.roots.push(Rc::clone(&row));
        row
    })
}
pub fn inherited_text_style(node: NodeId) -> TextStyle {
    context(node).text_style()
}

/// [`inherited_text_style`] for a leaf whose style is read before its node exists — a measured leaf builds its measure first — which resolves against the surface's root until it has one.
pub(crate) fn inherited_text_style_at(node: Option<NodeId>) -> TextStyle {
    match node {
        Some(node) => inherited_text_style(node),
        None => Inherited::initial().text_style(),
    }
}

#[cfg(test)]
#[path = "inherit_test.rs"]
mod tests;
