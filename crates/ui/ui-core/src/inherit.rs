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

    pub fn from_tokens(tokens: &dyn ThemeTokens) -> Self {
        let base = TextStyle::new(Self::BASE_FONT_SIZE, tokens.ink());
        Self {
            text: tokens.root().over(&base),
            cursor: Cursor::Default,
            direction: Direction::Ltr,
        }
    }

    pub fn with(&self, declared: &Declared) -> Self {
        Self {
            text: declared.over(&self.text),
            ..self.clone()
        }
    }

    pub fn text_style(&self) -> TextStyle {
        self.text.clone()
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
    /// Bumped only when nodes start or stop declaring, not on value changes, so a value write reruns only its own readers.
    structure: RwSignal<u64>,
    /// The nodes a [`ThemeProvider`](crate::ThemeProvider) put a theme of their own on. A walk stops at the nearest one and starts from that theme's root row, so a themed card inside a bar is set the way it would be in a window of its own, rather than in whatever the bar declared for its own text.
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

/// Records what `node` says for everything beneath it.
pub fn declare(node: NodeId, declared: Declared) {
    let existing = with_cascade_ref(|c| c.declared.get(&node).cloned());
    match (existing, declared.is_empty()) {
        (Some(sig), false) => {
            if sig.peek() != declared {
                sig.set(declared);
            }
        }
        (Some(_), true) => undeclare(node),
        (None, false) => {
            let structure = with_cascade(|c| {
                c.declared.insert(node, signal(declared));
                c.structure
            });
            // The owner that started declaring is what stops: tying withdrawal to the widget's `Drop` instead raced the path that frees layout nodes.
            reactive_core::on_cleanup(move || undeclare(node));
            structure.set(structure.peek().wrapping_add(1));
        }
        (None, true) => {}
    }
}

/// Forgets what `node` declared, for a node leaving the tree or stopping.
pub fn undeclare(node: NodeId) {
    let structure = with_cascade(|c| c.declared.remove(&node).map(|_| c.structure));
    if let Some(structure) = structure {
        structure.set(structure.peek().wrapping_add(1));
    }
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
    for current in layout_reactive::ancestors(node) {
        let (declared, themed) = with_cascade_ref(|c| {
            (
                c.declared.get(&current).copied(),
                c.themed.get(&current).map(|boundary| boundary.theme),
            )
        });
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

/// Reads the context inside the closure, not before, so the widget re-runs when a declaration above it moves rather than baking whatever was in force when it was built.
pub(crate) fn inheriting(
    node: NodeId,
    amend: impl Fn(TextStyle) -> TextStyle + 'static,
) -> Rc<dyn Fn() -> TextStyle> {
    Rc::new(move || amend(inherited_text_style(node)))
}

#[cfg(test)]
#[path = "inherit_test.rs"]
mod tests;
