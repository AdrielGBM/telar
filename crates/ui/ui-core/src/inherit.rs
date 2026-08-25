//! The properties that flow down the tree, and how a node finds the ones above it.
//!
//! Four systems used to decide what an undeclared text looks like — a literal baked into generated code, a
//! constructor argument, the catalogue's theme reads, and a widget's own ratio applied to those. Nothing
//! reconciled them, which is why a text field's label came out at 14.98px among 14px labels with no way to
//! say otherwise. This is the one table they collapse into: [`Inherited::initial`] holds the values that were
//! spread across those four places, and everything else is a node saying it wants something different.
//!
//! **Resolution is lazy, and that is a decision.** The obvious design resolves top-down in a pass before
//! layout — but the pass would have to run inside the layout engine, which sits *below* this crate and
//! cannot name these types. Resolving on read removes the ordering entirely: a style closure that resolves
//! when it is called is correct whenever it is called, so measurement sees resolved values by construction
//! rather than by anyone remembering to sequence a pass in front of it.
//!
//! What the eager design bought is kept by putting each declaration behind its own signal: a walk reads the
//! signals of the ancestors that actually declare, so a leaf ends up subscribed to those and nothing else,
//! and changing one re-runs exactly the leaves beneath it rather than every text on the surface.

use std::rc::Rc;

use layout_core::{Direction, NodeId};
use platform_core::Cursor;
use reactive_core::{RwSignal, signal};
use renderer_core::{Declared, TextStyle};
use rustc_hash::FxHashMap;
use theme_core::{ThemeTokens, use_theme_tokens};

/// Everything a node passes to its descendants.
///
/// `text` carries the inherited half of a text style. It never carries the *reset* half — `max_lines` and
/// `ellipsis`, which clamp one paragraph and would be nonsense applied to a subtree — and cannot: [`Declared`]
/// has no way to spell them, so the only thing that can modify this leaves them where [`Inherited::initial`]
/// put them.
#[derive(Clone, PartialEq, Debug)]
pub struct Inherited {
    pub text: TextStyle,
    pub cursor: Cursor,
    pub direction: Direction,
}

impl Inherited {
    /// The row an application that declares nothing renders against — today's defaults, in one place instead
    /// of four. `telar/tests/text_style_baseline.rs` asserts each of these against a real frame, so moving one
    /// is a decision rather than an accident.
    pub fn initial() -> Self {
        Self::from_tokens(&*use_theme_tokens())
    }

    /// The row `tokens` puts at the root of the tree.
    ///
    /// This is what makes a theme able to *set a property* rather than only supply a value: "the body text is
    /// 11px" stops being something every leaf has to be told and becomes one answer at the top. It is also
    /// where the size a `text` takes when nobody says otherwise now lives — a constant here and a token there
    /// were two numbers that happened to agree, and setting the theme moved only one of them.
    pub fn from_tokens(tokens: &dyn ThemeTokens) -> Self {
        Self {
            text: TextStyle::new(tokens.font_size(), tokens.ink()),
            cursor: Cursor::Default,
            direction: Direction::Ltr,
        }
    }

    /// This context with `declared` applied over it.
    pub fn with(&self, declared: &Declared) -> Self {
        Self {
            text: declared.over(&self.text),
            ..self.clone()
        }
    }

    /// The text style a leaf starts from, before anything it declares itself.
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
    /// Per surface, because a texture UI and the window around it are different documents: one at 320×180
    /// declaring `raster:pixel` must not reach into the chrome beside it.
    slot CASCADE: Cascade = Cascade::default();
    access with_cascade, with_cascade_ref;
    context CascadeContext, CascadeGuard;
}

struct Cascade {
    /// What each declaring node says, behind a signal so reading it *subscribes* the widget that read it.
    /// Empty until markup can declare anything, which is what makes every walk below cost one map miss per
    /// ancestor until then.
    declared: FxHashMap<NodeId, RwSignal<Declared>>,
    /// Bumped when the *set* of declaring nodes changes, not when one of their values does.
    ///
    /// Reading it subscribes every context read to "somebody started or stopped declaring", which is what
    /// lets a container declare *after* the leaf below it has already rendered — the order a tree is built
    /// in. A value change does not go through here: it sets that one node's signal, so only the leaves that
    /// actually read it re-run, which is the property a single global epoch would have thrown away.
    structure: RwSignal<u64>,
    /// The last row the theme resolved to, kept so nodes that resolve to the same value share it rather than
    /// each holding a copy. Replaced only when the value actually differs — a theme swap and a light/dark
    /// flip both change it, and neither is something the cascade can be told about.
    root: Rc<Inherited>,
}

impl Default for Cascade {
    fn default() -> Self {
        Self {
            declared: FxHashMap::default(),
            structure: signal(0),
            root: Rc::new(Inherited::initial()),
        }
    }
}

/// Forgets every declaration, for a tree being replaced wholesale. See
/// [`reset_layout_runtime`](crate::context::reset_layout_runtime) for why it cannot be done separately.
pub(crate) fn reset_cascade() {
    with_cascade(|c| *c = Cascade::default());
}

/// Records what `node` says for everything beneath it.
pub fn declare(node: NodeId, declared: Declared) {
    let existing = with_cascade_ref(|c| c.declared.get(&node).cloned());
    match (existing, declared.is_empty()) {
        // A value change: only what read this node's signal re-runs.
        (Some(sig), false) => {
            if sig.peek() != declared {
                sig.set(declared);
            }
        }
        (Some(_), true) => undeclare(node),
        (None, false) => {
            let structure = with_cascade(|c| {
                c.declared.insert(node, signal(declared));
                c.structure.clone()
            });
            structure.set(structure.peek().wrapping_add(1));
        }
        (None, true) => {}
    }
}

/// Forgets what `node` declared, for a node leaving the tree or stopping.
pub fn undeclare(node: NodeId) {
    let structure = with_cascade(|c| c.declared.remove(&node).map(|_| c.structure.clone()));
    if let Some(structure) = structure {
        structure.set(structure.peek().wrapping_add(1));
    }
}

/// The context in force at `node` — everything its ancestors declared, merged in tree order.
///
/// Walks to the root, reading each declaring ancestor's signal on the way, so the caller ends up subscribed
/// to exactly the declarations that are actually above it and to nothing else. A tree where nothing declares
/// — every tree, until markup can — walks a handful of map misses and returns the one shared root value
/// without allocating.
pub fn context(node: NodeId) -> Rc<Inherited> {
    let root = root();
    let structure = with_cascade_ref(|c| c.structure.clone());
    // A node that declares nothing has no signal to read, and it is exactly that node that may start.
    let _ = structure.get();

    let mut chain: Vec<Declared> = Vec::new();
    let mut at = Some(node);
    while let Some(current) = at {
        if let Some(sig) = with_cascade_ref(|c| c.declared.get(&current).cloned()) {
            chain.push(sig.get());
        }
        at = layout_reactive::parent(current);
    }
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

/// The row the theme puts at the top of the tree.
///
/// Resolved on read for the same reason the rest of the cascade is: the theme is a signal, so reading it here
/// subscribes whatever asked, and a mode switch repaints exactly the text that took a colour from it. The
/// value is compared rather than the theme handle, because the built-in answers follow the light/dark mode
/// without the handle ever changing.
fn root() -> Rc<Inherited> {
    let resolved = Inherited::initial();
    with_cascade(|c| {
        if *c.root != resolved {
            c.root = Rc::new(resolved);
        }
        Rc::clone(&c.root)
    })
}

/// The text style a leaf at `node` inherits, before anything it declares itself.
pub fn inherited_text_style(node: NodeId) -> TextStyle {
    context(node).text_style()
}

/// A style closure that resolves against what `node` inherits, amended by `amend`.
///
/// The shape every leaf that inherits needs, in one place: read the context *inside* the closure, so the
/// widget re-runs when a declaration above it moves rather than baking whatever was in force when it was
/// built.
pub(crate) fn inheriting(
    node: NodeId,
    amend: impl Fn(TextStyle) -> TextStyle + 'static,
) -> Rc<dyn Fn() -> TextStyle> {
    Rc::new(move || amend(inherited_text_style(node)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::reset_layout_runtime;
    use layout_core::LayoutStyle;

    fn tree() -> (NodeId, NodeId, NodeId) {
        reset_layout_runtime();
        with_cascade(|c| *c = Cascade::default());
        let (leaf, _) = layout_reactive::new_leaf(LayoutStyle::new()).unwrap();
        let inner = layout_reactive::new_container(LayoutStyle::new(), &[leaf]).unwrap();
        let outer = layout_reactive::new_container(LayoutStyle::new(), &[inner]).unwrap();
        (outer, inner, leaf)
    }

    /// The state every tree is in until markup can declare anything: nothing is declared, so every node
    /// resolves to the same initial row and shares one value.
    #[test]
    fn an_undeclared_tree_resolves_every_node_to_the_initial_row() {
        let (outer, inner, leaf) = tree();
        let initial = Inherited::initial();
        for node in [outer, inner, leaf] {
            assert_eq!(*context(node), initial);
        }
        assert!(
            Rc::ptr_eq(&context(outer), &context(leaf)),
            "nodes resolving to the same value must share it rather than each holding a copy"
        );
    }

    /// The point of the whole thing: an ancestor that draws no text of its own still says what the text
    /// beneath it looks like, the way `body { font-size }` does for a body that draws none.
    #[test]
    fn a_declaration_reaches_a_leaf_that_did_not_ask_for_it() {
        let (outer, _, leaf) = tree();
        declare(outer, Declared::default().with_font_size(11.0));
        assert_eq!(context(leaf).text.font_size, 11.0);
        assert_eq!(
            context(leaf).text.font_weight,
            Inherited::initial().text.font_weight,
            "a declaration says nothing about the properties it did not name"
        );
    }

    /// Nearer wins, which is the whole of a cascade.
    #[test]
    fn the_nearest_declaration_wins() {
        let (outer, inner, leaf) = tree();
        declare(outer, Declared::default().with_font_size(11.0));
        declare(inner, Declared::default().with_font_size(22.0));
        assert_eq!(context(leaf).text.font_size, 22.0);
        assert_eq!(context(outer).text.font_size, 11.0);
    }

    /// Two declarations at different depths compose rather than replace: the inner one names a size and
    /// inherits the outer one's weight without restating it.
    #[test]
    fn declarations_compose_down_the_tree() {
        let (outer, inner, leaf) = tree();
        declare(outer, Declared::default().with_font_weight(700));
        declare(inner, Declared::default().with_font_size(22.0));
        let at_leaf = context(leaf);
        assert_eq!(at_leaf.text.font_weight, 700);
        assert_eq!(at_leaf.text.font_size, 22.0);
    }

    /// A remount builds a whole new tree, and the runtime hands out the same `NodeId`s again. A declaration
    /// that outlived its tree would land on whatever is built on that id next — text the wrong size under a
    /// node nobody declared for, which is not a failure anything reports.
    #[test]
    fn a_new_tree_inherits_nothing_from_the_one_it_replaced() {
        let (outer, _, _) = tree();
        declare(outer, Declared::default().with_font_size(11.0));
        assert_eq!(context(outer).text.font_size, 11.0);

        // What a remount does, and only that: `tree()` clears the cascade by hand, which is the very thing
        // being tested. The same shape is rebuilt so the ids come back in the same order.
        crate::context::reset_layout_runtime();
        let (leaf, _) = layout_reactive::new_leaf(LayoutStyle::new()).unwrap();
        let inner = layout_reactive::new_container(LayoutStyle::new(), &[leaf]).unwrap();
        let again = layout_reactive::new_container(LayoutStyle::new(), &[inner]).unwrap();

        assert_eq!(
            again, outer,
            "the id was recycled, which is the whole hazard"
        );
        assert_eq!(
            context(leaf).text.font_size,
            Inherited::initial().text.font_size,
            "and the new tree inherits nothing from the one it replaced"
        );
    }

    /// The reason the root is not a constant: a theme saying "the body text is 11px" says it once, at the
    /// top, instead of every leaf having to be told.
    #[test]
    fn the_theme_is_what_sits_at_the_root() {
        #[derive(Clone)]
        struct Small;
        impl ThemeTokens for Small {
            fn font_size(&self) -> f32 {
                11.0
            }
        }

        /// Answers nothing, so it restores the built-in row — and swapping back is the other half of what is
        /// being tested, since a root cached against a stale theme would keep serving 11.
        #[derive(Clone)]
        struct Silent;
        impl ThemeTokens for Silent {}

        let (_, _, leaf) = tree();
        let built_in = context(leaf).text.font_size;
        assert_ne!(built_in, 11.0, "the built-in row is not already 11");

        theme_core::set_theme(Small);
        assert_eq!(context(leaf).text.font_size, 11.0);

        theme_core::set_theme(Silent);
        assert_eq!(context(leaf).text.font_size, built_in);
    }

    /// A memo that outlived the declaration it came from would serve the old value forever, which is the one
    /// way lazy resolution can be wrong.
    #[test]
    fn changing_a_declaration_is_visible_to_everything_under_it() {
        let (outer, _, leaf) = tree();
        declare(outer, Declared::default().with_font_size(11.0));
        assert_eq!(context(leaf).text.font_size, 11.0);
        declare(outer, Declared::default().with_font_size(9.0));
        assert_eq!(context(leaf).text.font_size, 9.0);
        declare(outer, Declared::default());
        assert_eq!(
            context(leaf).text.font_size,
            Inherited::initial().text.font_size
        );
    }
}
