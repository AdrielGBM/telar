//! [`PreviewEntry`]: everything the runner, the test harness and the workshop know about one preview.

use reactive_core::RwSignal;

use crate::{
    Children, Color, Container, Direction, LayoutError, LayoutItem, LayoutStyle, Size, Slots,
};

use super::host::Args;
use super::{ActionLog, ArgSpec, Globals, Matrix, PlayFn, PreviewArg, PropsSchema};

/// What a preview's build fn is handed: the canvas it is built for, which it reads its args from, logs its callbacks' calls to and takes its environment from.
///
/// A host makes one for an entry with [`PreviewCtx::for_entry`], or from the canvas's [`Args`] with `PreviewCtx::from`, and hands it the action log and the globals it shares with its panels through [`with_actions`](Self::with_actions) and [`with_globals`](Self::with_globals). `PreviewCtx::default` holds everything in memory only.
#[derive(Clone, Debug, Default)]
pub struct PreviewCtx {
    args: Args,
    actions: ActionLog,
    globals: Globals,
}

impl PreviewCtx {
    /// The canvas `entry` mounts in: its args persisted under its id, a fresh action log, and globals seeded from its [`PreviewEnv`].
    pub fn for_entry(entry: &PreviewEntry) -> Self {
        Self::from(Args::for_entry(entry)).with_globals(Globals::seeded(&entry.env))
    }

    pub fn with_actions(self, actions: ActionLog) -> Self {
        Self { actions, ..self }
    }

    pub fn with_globals(self, globals: Globals) -> Self {
        Self { globals, ..self }
    }

    /// The log the preview's callbacks record their calls in.
    pub fn actions(&self) -> ActionLog {
        self.actions
    }

    /// The environment the canvas is given.
    pub fn globals(&self) -> Globals {
        self.globals
    }

    /// The value `name`'s control holds, or `default`, in `default`'s own type, so a setter coerces it exactly as it would the literal.
    ///
    /// Reading an arg this way registers it as one whose change builds the preview again: a value handed to a prop once, at build, cannot follow a control any other way. Use [`Self::signal`] for a value the tree reads as it runs.
    pub fn arg<T: PreviewArg>(&self, name: &'static str, default: T) -> T {
        self.args.arg(name, default)
    }

    /// A signal shared with `name`'s control: a control's change reaches the tree as it runs, without building it again, and the tree's writes reach the control.
    ///
    /// The same signal comes back for as long as the canvas lives, so its value survives the preview being built again for another arg.
    pub fn signal<T: PreviewArg>(&self, name: &'static str, default: T) -> RwSignal<T> {
        self.args.signal(name, default)
    }

    /// The args behind this canvas, which a host's controls list and set.
    pub fn args(&self) -> &Args {
        &self.args
    }
}

impl From<Args> for PreviewCtx {
    fn from(args: Args) -> Self {
        Self {
            args,
            actions: ActionLog::new(),
            globals: Globals::new(),
        }
    }
}

/// Builds a preview's tree against the canvas it is mounted in.
pub type BuildFn = fn(&PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError>;

/// Wraps a preview's root, handed over as the children of whatever the decorator builds: a component call with its props fixed, `|children| card(CardProps::props().build(), children)`.
///
/// The root is built where the decorator places its children, so it sees every theme and context the decorator provides.
pub type Decorator = fn(Children) -> Result<Box<dyn LayoutItem>, LayoutError>;

/// How the canvas places a preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// Top-start, with the canvas's padding around it.
    #[default]
    Padded,
    /// Centred in the canvas, at its own size.
    Centered,
    /// Filling the canvas edge to edge, with no frame.
    Fullscreen,
}

/// What a preview's canvas is given by a compositor when the preview is a *surface* rather than a tree — `[preview "…" surface:360x240]`.
///
/// A tree preview answers "does this component look right"; a surface preview answers "does this *window* look right": a definite size the content lays out against, and the enter transition its root plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewSurface {
    /// The size the compositor would give the surface, in logical px.
    pub width: f32,
    pub height: f32,
    /// Play the root's enter transition, so a transition that never settles shows up as a window still half-transparent when the frames run out.
    pub animate: bool,
}

impl PreviewSurface {
    pub const fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            animate: false,
        }
    }

    pub const fn animated(self) -> Self {
        Self {
            animate: true,
            ..self
        }
    }

    /// `content` given the two things a compositor would give a surface: a definite size to lay out against, and the root that plays its enter transition.
    ///
    /// The size goes on a box around the root rather than on the root itself: [`crate::WindowRoot::wrapping`] fills its parent by design, which is how a surface's content stretches to its window, so it needs a parent with a size.
    fn mount(self, content: Box<dyn LayoutItem>) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let root = crate::WindowRoot::wrapping(content)?;
        let root = if self.animate {
            root.animate_in()
        } else {
            root
        };
        Ok(Box::new(Container::new(
            LayoutStyle::new().width(self.width).height(self.height),
            vec![Box::new(root) as Box<dyn LayoutItem>],
        )?))
    }
}

/// The environment a preview asks its canvas for. `None` leaves the canvas's own setting in place.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewEnv {
    /// The canvas size, in logical px.
    pub viewport: Option<Size>,
    pub background: Option<Color>,
    /// A mode registered with `register_mode`, by id.
    pub mode: Option<&'static str>,
    /// A BCP 47 language tag.
    pub locale: Option<&'static str>,
    pub direction: Option<Direction>,
}

impl PreviewEnv {
    pub const NONE: Self = Self {
        viewport: None,
        background: None,
        mode: None,
        locale: None,
        direction: None,
    };

    pub const fn viewport(self, width: f32, height: f32) -> Self {
        Self {
            viewport: Some(Size::new(width, height)),
            ..self
        }
    }

    pub const fn background(self, color: Color) -> Self {
        Self {
            background: Some(color),
            ..self
        }
    }

    pub const fn mode(self, mode: &'static str) -> Self {
        Self {
            mode: Some(mode),
            ..self
        }
    }

    pub const fn locale(self, locale: &'static str) -> Self {
        Self {
            locale: Some(locale),
            ..self
        }
    }

    pub const fn direction(self, direction: Direction) -> Self {
        Self {
            direction: Some(direction),
            ..self
        }
    }
}

/// A byte range of [`PreviewEntry::source`] and the 1-based line of the source file it starts on.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: u32,
    pub end: u32,
    pub line: u32,
}

impl SourceSpan {
    pub const fn new(start: u32, end: u32, line: u32) -> Self {
        Self { start, end, line }
    }
}

/// One preview: where it is written, how its canvas is set up, and the fn that builds it.
///
/// Built with [`PreviewEntry::new`] and the `const` setters, so a generated table stays a `const` and a field added later is not a breaking change. A Rust preview starts from [`preview!`](super::preview) instead, which fills in the id, the location, the source and the props, and goes on with the same setters.
#[non_exhaustive]
#[derive(Clone, Copy)]
pub struct PreviewEntry {
    /// `<crate>--<component>--<slug(name)>`: stable across builds, and what deep links and snapshot files are named by.
    pub id: &'static str,
    /// The `Group/Title` path the preview is listed under. Defaults to [`Self::component`].
    pub title: &'static str,
    pub component: &'static str,
    pub name: &'static str,
    /// The absolute path of the file the preview is written in, as an editor opens it: its package's `CARGO_MANIFEST_DIR` joined to the path within the package, for a `.rsx` preview and a Rust one alike. Empty when unknown.
    pub file: &'static str,
    /// 1-based line of the preview in [`Self::file`], or 0 when unknown.
    pub line: u32,
    /// The preview's own source text, shown beside the canvas. Empty when not captured.
    pub source: &'static str,
    pub source_spans: &'static [SourceSpan],
    pub tags: &'static [&'static str],
    pub layout: Layout,
    pub env: PreviewEnv,
    /// Mount the preview the way a runner mounts a surface rather than as a tree.
    pub surface: Option<PreviewSurface>,
    pub matrix: Option<Matrix>,
    /// The args the preview declares, each of which gets a control.
    pub args: &'static [ArgSpec],
    /// Builds the bare tree. A host mounts [`Self::build_root`] instead, which applies [`Self::decorator`] and [`Self::surface`].
    pub build: BuildFn,
    pub decorator: Option<Decorator>,
    pub play: Option<PlayFn>,
    /// The props of the component under preview, for its docs and controls.
    pub props: Option<fn() -> &'static PropsSchema>,
}

impl PreviewEntry {
    pub const fn new(
        id: &'static str,
        component: &'static str,
        name: &'static str,
        build: BuildFn,
    ) -> Self {
        Self {
            id,
            title: component,
            component,
            name,
            file: "",
            line: 0,
            source: "",
            source_spans: &[],
            tags: &[],
            layout: Layout::Padded,
            env: PreviewEnv::NONE,
            surface: None,
            matrix: None,
            args: &[],
            build,
            decorator: None,
            play: None,
            props: None,
        }
    }

    /// The `Group/Title` path to list the preview under.
    pub const fn title(self, title: &'static str) -> Self {
        Self { title, ..self }
    }

    pub const fn location(self, file: &'static str, line: u32) -> Self {
        Self { file, line, ..self }
    }

    pub const fn source(self, source: &'static str, spans: &'static [SourceSpan]) -> Self {
        Self {
            source,
            source_spans: spans,
            ..self
        }
    }

    /// Free-form labels the workshop filters by.
    pub const fn tags(self, tags: &'static [&'static str]) -> Self {
        Self { tags, ..self }
    }

    pub const fn layout(self, layout: Layout) -> Self {
        Self { layout, ..self }
    }

    pub const fn env(self, env: PreviewEnv) -> Self {
        Self { env, ..self }
    }

    /// The canvas size, in logical px.
    pub const fn viewport(self, width: f32, height: f32) -> Self {
        let env = self.env.viewport(width, height);
        Self { env, ..self }
    }

    /// The canvas background. Named `bg` after the `.rsx` preview header's key.
    pub const fn bg(self, color: Color) -> Self {
        let env = self.env.background(color);
        Self { env, ..self }
    }

    /// A mode registered with `register_mode`, by id.
    pub const fn mode(self, mode: &'static str) -> Self {
        let env = self.env.mode(mode);
        Self { env, ..self }
    }

    /// A BCP 47 language tag.
    pub const fn locale(self, locale: &'static str) -> Self {
        let env = self.env.locale(locale);
        Self { env, ..self }
    }

    /// The writing direction, whatever the locale's own. Named `dir` after the `.rsx` preview header's key.
    pub const fn dir(self, direction: Direction) -> Self {
        let env = self.env.direction(direction);
        Self { env, ..self }
    }

    pub const fn surface(self, surface: PreviewSurface) -> Self {
        Self {
            surface: Some(surface),
            ..self
        }
    }

    /// Renders the preview once per cell of `matrix`: `.matrix(Matrix::Named("themes"))`.
    pub const fn matrix(self, matrix: Matrix) -> Self {
        Self {
            matrix: Some(matrix),
            ..self
        }
    }

    pub const fn args(self, args: &'static [ArgSpec]) -> Self {
        Self { args, ..self }
    }

    /// Scripts an interaction against the mounted preview: `.play(|canvas| …)`.
    pub const fn play(self, play: PlayFn) -> Self {
        Self {
            play: Some(play),
            ..self
        }
    }

    pub const fn props(self, props: fn() -> &'static PropsSchema) -> Self {
        Self {
            props: Some(props),
            ..self
        }
    }

    /// Wraps the root in `decorator`, replacing any decorator set before. Named `decorate` after the `.rsx` preview header's `decorator` key.
    pub const fn decorate(self, decorator: Decorator) -> Self {
        Self {
            decorator: Some(decorator),
            ..self
        }
    }

    /// Builds the preview against `ctx`, inside its decorator when it has one, and mounted as a surface when it is one: what every host mounts.
    pub fn build_root(&self, ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let root = match self.decorator {
            None => (self.build)(ctx)?,
            Some(decorator) => {
                let build = self.build;
                let ctx = ctx.clone();
                decorator(Children::new(move || {
                    let mut slots = Slots::new();
                    slots.push(None, build(&ctx)?);
                    Ok(slots)
                }))?
            }
        };
        match self.surface {
            None => Ok(root),
            Some(surface) => surface.mount(root),
        }
    }
}

impl std::fmt::Debug for PreviewEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewEntry")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("file", &self.file)
            .field("line", &self.line)
            .finish_non_exhaustive()
    }
}
