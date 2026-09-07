//! The properties that flow down the tree, and how a node finds the ones above it.
//!
//! Four systems used to decide what an undeclared text looks like — a literal baked into generated code, a constructor argument, the catalogue's theme reads, and a widget's own ratio applied to those. Nothing reconciled them, which is why a text field's label came out at 14.98px among 14px labels with no way to say otherwise. This is the one table they collapse into: [`Inherited::initial`] holds the values that were spread across those four places, and everything else is a node saying it wants something different.
//!
//! **Two types, because they answer different questions.** [`Inherited`] is a *complete* row — every property has a value here, which is what a leaf needs to draw. [`Declared`](renderer_core::Declared) is a *partial* one, every field an `Option`, which is what a node saying "bold from here down" needs and what a complete style could never express: a whole `TextStyle` carries a value for every field it did not mean to change, and overwrites with it. The same partial type is what a byte range of a paragraph uses, which is not a coincidence — a span is a cascade child whose extent is a range instead of a subtree.
//!
//! **Resolution is lazy, and that is a decision.** The obvious design resolves top-down in a pass before layout — but the pass would have to run inside the layout engine, which sits *below* this crate and cannot name these types. Resolving on read removes the ordering entirely: a style closure that resolves when it is called is correct whenever it is called, so measurement sees resolved values by construction rather than by anyone remembering to sequence a pass in front of it.
//!
//! What the eager design bought is kept by putting each declaration behind its own signal: a walk reads the signals of the ancestors that actually declare, so a leaf ends up subscribed to those and nothing else, and changing one re-runs exactly the leaves beneath it rather than every text on the surface.
//!
//! **The key is a layout `NodeId` and the walk is layout-shaped, while the *lifetime* is an owner's.** Those answer different questions and it is worth saying why they are allowed to disagree. Inheritance follows the document, so the walk has to climb the layout parent chain — a component boundary is not a document boundary, and children handed in through a slot are built under a different owner but inherit from where the markup put them. Re-keying on owners would silently change what inherits from what. Withdrawal is the opposite: it used to be the declaring widget's `Drop`, which raced the independent path that frees layout nodes, and a replaced layout runtime hands the next tree the same ids. So `declare` registers its own withdrawal on the owner active at the time, and disposal runs it in order.

use std::rc::Rc;

use layout_core::{Direction, NodeId};
use platform_core::Cursor;
use reactive_core::{RwSignal, signal};
use renderer_core::{Declared, TextStyle};
use rustc_hash::FxHashMap;
use theme_core::{ThemeTokens, use_theme_tokens};

/// Everything a node passes to its descendants.
///
/// `text` carries the inherited half of a text style. It never carries the *reset* half — `max_lines` and `ellipsis`, which clamp one paragraph and would be nonsense applied to a subtree — and cannot: [`Declared`] has no way to spell them, so the only thing that can modify this leaves them where [`Inherited::initial`] put them.
#[derive(Clone, PartialEq, Debug)]
pub struct Inherited {
    pub text: TextStyle,
    pub cursor: Cursor,
    pub direction: Direction,
}

impl Inherited {
    /// The row an application that declares nothing renders against — today's defaults, in one place instead of four. `telar/src/text_style_baseline_test.rs` asserts each of these against a real frame, so moving one is a decision rather than an accident.
    pub fn initial() -> Self {
        Self::from_tokens(&*use_theme_tokens())
    }

    /// The size a document is set in before a theme or any markup has said otherwise.
    ///
    /// A constant here rather than a theme token: text size *inherits*, so a theme that wants to move it declares it at [`ThemeTokens::root`] like any other inherited property, and there is no second channel a component can read past the cascade to reach.
    pub const BASE_FONT_SIZE: f32 = 14.0;

    /// The row `tokens` puts at the root of the tree.
    ///
    /// This is what makes a theme able to *set a property* rather than only supply a value: "the body text is 11px" stops being something every leaf has to be told and becomes one answer at the top. It is also where the size a `text` takes when nobody says otherwise now lives — a constant here and a token there were two numbers that happened to agree, and setting the theme moved only one of them.
    pub fn from_tokens(tokens: &dyn ThemeTokens) -> Self {
        let base = TextStyle::new(Self::BASE_FONT_SIZE, tokens.ink());
        Self {
            text: tokens.root().over(&base),
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
    /// Per surface, because a texture UI and the window around it are different documents: one at 320×180 declaring `raster:pixel` must not reach into the chrome beside it.
    slot CASCADE: Cascade = Cascade::default();
    access with_cascade, with_cascade_ref;
    context CascadeContext, CascadeGuard;
}

struct Cascade {
    /// What each declaring node says, behind a signal so reading it *subscribes* the widget that read it. Empty until markup can declare anything, which is what makes every walk below cost one map miss per ancestor until then.
    declared: FxHashMap<NodeId, RwSignal<Declared>>,
    /// Bumped when the *set* of declaring nodes changes, not when one of their values does.
    ///
    /// Reading it subscribes every context read to "somebody started or stopped declaring", which is what lets a container declare *after* the leaf below it has already rendered — the order a tree is built in. A value change does not go through here: it sets that one node's signal, so only the leaves that actually read it re-run, which is the property a single global epoch would have thrown away.
    structure: RwSignal<u64>,
    /// The last row the theme resolved to, kept so nodes that resolve to the same value share it rather than each holding a copy. Replaced only when the value actually differs — a theme swap and a light/dark flip both change it, and neither is something the cascade can be told about.
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

/// Forgets every declaration, for a tree being replaced wholesale. See [`reset_layout_runtime`](crate::context::reset_layout_runtime) for why it cannot be done separately.
pub(crate) fn reset_cascade() {
    // Detached because this rebuilds the surface's cascade world, whose signals outlive every scope: attributing them to whatever owner is resetting would free them with it.
    with_cascade(|c| *c = reactive_core::detached(Cascade::default));
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
                c.structure
            });
            // The owner that started declaring is what stops. The declaring widget's `Drop` raced the independent path that frees layout nodes, which cost text at the wrong size.
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

/// The context in force at `node` — everything its ancestors declared, merged in tree order.
///
/// Walks to the root, reading each declaring ancestor's signal on the way, so the caller ends up subscribed to exactly the declarations that are actually above it and to nothing else. A tree where nothing declares — every tree, until markup can — walks a handful of map misses and returns the one shared root value without allocating.
pub fn context(node: NodeId) -> Rc<Inherited> {
    let root = root();
    let structure = with_cascade_ref(|c| c.structure);
    // A node that declares nothing has no signal to read, and it is exactly that node that may start.
    let _ = structure.get();

    let mut chain: Vec<Declared> = Vec::new();
    for current in layout_reactive::ancestors(node) {
        if let Some(sig) = with_cascade_ref(|c| c.declared.get(&current).cloned()) {
            chain.push(sig.get());
        }
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
/// Resolved on read for the same reason the rest of the cascade is: the theme is a signal, so reading it here subscribes whatever asked, and a mode switch repaints exactly the text that took a colour from it. The value is compared rather than the theme handle, because the built-in answers follow the light/dark mode without the handle ever changing.
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
/// The shape every leaf that inherits needs, in one place: read the context *inside* the closure, so the widget re-runs when a declaration above it moves rather than baking whatever was in force when it was built.
pub(crate) fn inheriting(
    node: NodeId,
    amend: impl Fn(TextStyle) -> TextStyle + 'static,
) -> Rc<dyn Fn() -> TextStyle> {
    Rc::new(move || amend(inherited_text_style(node)))
}

#[cfg(test)]
#[path = "inherit_test.rs"]
mod tests;
