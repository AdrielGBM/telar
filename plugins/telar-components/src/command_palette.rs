//! [`command_palette`]: a dialog that finds a command by typing part of its name and runs it, with the keyboard never leaving the search field.

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Clip, ClippedItem, Color, Container,
    Declared, JustifyContent, Key, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Memo,
    ModifiersState, NamedKey, Props, Reactive, ReactiveList, RectStyle, RwSignal, ShapeStyle, Span,
    StyledContainer, SurfaceAmendment, SurfaceStyle, Text, TextWrap, amend_surface, box_item,
    children, effect,
    focus::{FocusId, Role},
    memo, signal,
};

use crate::fuzzy::fuzzy_match;
use crate::list::{ListTokens, row_paint, tinted};
use crate::rows::{BuiltRows, RowPlaces};
use crate::scrim;
use crate::shared::{self, shown_chord};
use crate::strings;
use crate::text_field::{TextFieldProps, text_field};
use crate::{KbdProps, kbd};

/// How many rows show before the list scrolls.
const VISIBLE_ROWS: usize = 8;
const OVERSCAN: usize = 4;
/// What a match against a keyword or the group's name gives up to one against the command's own name.
const INDIRECT_PENALTY: i32 = 6;
/// How many ems wide the dialog is when its tokens name no width: room for a long command's name, its group's heading and its shortcut on one line.
const WIDTH_EMS: f32 = 40.0;
/// How far down the window the dialog stands when its tokens say nothing, in the theme's widest spacing step: high enough that the list has the room below it to grow into.
const INSET_IN_GAPS: f32 = 6.0;

/// The command palette's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the palette opens. Its rows follow [`ListTokens`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CommandPaletteTokens {
    /// Width of the dialog at the regular control size, scaled with the control size in force. `None` is 40 ems of the document's text. A palette's own `width` prop wins over it.
    pub width: Option<f32>,
    /// Corner of the dialog. `None` is the theme's `radius_dialog`.
    pub radius: Option<f32>,
    /// How far below the top of the window the dialog stands, at the regular control size and scaled with the control size in force. `None` is six of [`metrics::gap_xl`](crate::metrics::gap_xl).
    pub inset_top: Option<f32>,
}

impl CommandPaletteTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn width(&self) -> f32 {
        shared::width_or_measure(self.width, WIDTH_EMS)
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_dialog)
    }

    pub fn inset_top(&self) -> f32 {
        self.inset_top.map_or_else(
            || shared::gap_xl() * INSET_IN_GAPS,
            |inset| inset * telar::control_scale(),
        )
    }
}

fn card_pad() -> f32 {
    shared::gap_md()
}

/// One thing the palette can run: what it is called, the group it is listed under, the other words it answers to and the shortcut that runs it without the palette.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Command {
    /// Unique among the palette's commands: what [`CommandPaletteProps::on_run`] tells them apart by.
    pub id: Arc<str>,
    pub label: Arc<str>,
    pub group: Option<Arc<str>>,
    /// Words the command is found by that its name does not say: `"theme"` for "Toggle dark mode".
    pub keywords: Arc<[Arc<str>]>,
    /// The chord shown beside it, written portably as [`kbd`](crate::kbd) takes it: `"Mod+K"`.
    pub shortcut: Option<Arc<str>>,
    /// Listed, but passed over by the keyboard and never run.
    pub disabled: bool,
}

impl Command {
    pub fn new(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            group: None,
            keywords: Arc::new([]),
            shortcut: None,
            disabled: false,
        }
    }

    pub fn in_group(mut self, group: impl Into<Arc<str>>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_keywords<K: Into<Arc<str>>>(
        mut self,
        keywords: impl IntoIterator<Item = K>,
    ) -> Self {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_shortcut(mut self, chord: impl Into<Arc<str>>) -> Self {
        self.shortcut = Some(chord.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// What [`CommandPaletteProps::on_run`] is called with.
pub type RunHandler = Rc<dyn Fn(&Command)>;

/// A dialog over the page with a search field and, under it, the commands that match what was typed, best first and listed under their groups.
///
/// The keyboard stays in the field: the arrows walk the list, Enter runs the command the cursor is on, Escape closes. A reader hears the row the cursor is on as it moves, because the field points at it.
#[derive(Props)]
pub struct CommandPaletteProps {
    /// Bound open state. `None` (the default) leaves it to [`id`](Self::id), and with neither it never opens.
    #[props(some, into, default)]
    pub open: Option<RwSignal<bool>>,
    /// Names the palette, so anything on the same surface can open it with `open_overlay(id)` without holding its signal; another surface has its own `id`. Ignored when `open` is bound.
    #[props(default = "")]
    pub id: &'static str,
    #[props(into, default)]
    pub commands: Reactive<Vec<Command>>,
    /// Bound search text. `None` (the default) is uncontrolled. It is cleared each time the palette closes, so it opens on the whole list unless something sets it first.
    #[props(some, into, default)]
    pub query: Option<RwSignal<String>>,
    /// The field's hint. `None` (the default) uses the localized `telar_components.search_commands` message.
    #[props(some, into, default)]
    pub placeholder: Option<Reactive<String>>,
    /// What a reader calls the dialog. `None` (the default) uses the localized `telar_components.command_palette` message.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// What shows when nothing matches. `None` (the default) uses the localized `telar_components.no_results` message.
    #[props(some, into, default)]
    pub empty_label: Option<Reactive<String>>,
    /// Fired with the command a press or Enter ran, after the palette has closed.
    #[props(some, default)]
    pub on_run: Option<RunHandler>,
    /// Runs whenever the palette closes, whether a command ran or not.
    #[props(some, default)]
    pub on_close: Option<Rc<dyn Fn()>>,
    /// The dialog's width in logical px. `0.0` (the default) is [`CommandPaletteTokens::width`].
    #[props(default)]
    pub width: f32,
    /// The colour the matched characters are drawn in and the cursor's wash is tinted with. `Color::TRANSPARENT` (the default) draws the matches in the theme accent and leaves the theme's wash as it is.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the dialog — this component's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`CommandPaletteTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A dialog that finds a command by typing part of its name, and runs it.
pub fn command_palette(
    props: CommandPaletteProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let CommandPaletteProps {
        open,
        id,
        commands,
        query,
        placeholder,
        label,
        empty_label,
        on_run,
        on_close,
        width,
        color,
        style,
    } = props;
    let open = shared::resolve_open(open, id);
    let query = query.unwrap_or_else(|| signal(String::new()));
    let parts = Parts {
        commands,
        query,
        placeholder,
        label,
        empty_label,
        on_run,
        width,
        color,
        style,
    };
    scrim::scrim_overlay(open, on_close, move |dismiss| {
        build_open_palette(parts, open, dismiss)
    })
}

struct Parts {
    commands: Reactive<Vec<Command>>,
    query: RwSignal<String>,
    placeholder: Option<Reactive<String>>,
    label: Option<Reactive<String>>,
    empty_label: Option<Reactive<String>>,
    on_run: Option<RunHandler>,
    /// `0.0` for the tokens' width.
    width: f32,
    color: Reactive<Color>,
    style: SurfaceStyle,
}

/// One line of the list: a group's heading, or a command with the ranges of its name the query matched.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Entry {
    Heading(Arc<str>),
    Item {
        command: Command,
        marks: Vec<Range<usize>>,
    },
}

impl Entry {
    fn reachable_id(&self) -> Option<&Arc<str>> {
        match self {
            Entry::Item { command, .. } if !command.disabled => Some(&command.id),
            _ => None,
        }
    }
}

/// The ranked list. Equal only to itself, so the memo notifies on every re-rank rather than comparing whole lists.
#[derive(Clone)]
struct Results(Rc<Vec<Rc<Entry>>>);

impl PartialEq for Results {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// What `query` makes of `commands`: with nothing typed, every command in the order given under its group; otherwise only the ones that match, best first, each group led by its best match.
fn rank(commands: &[Command], query: &str) -> Vec<Entry> {
    let blank = query.trim().is_empty();
    let mut buckets: Vec<Bucket> = Vec::new();
    for (order, command) in commands.iter().enumerate() {
        let Some((score, marks)) = score(command, query) else {
            continue;
        };
        let ranked = Ranked {
            score,
            order,
            entry: Entry::Item {
                command: command.clone(),
                marks,
            },
        };
        match buckets
            .iter_mut()
            .find(|bucket| bucket.group == command.group)
        {
            Some(bucket) => bucket.items.push(ranked),
            None => buckets.push(Bucket {
                group: command.group.clone(),
                items: vec![ranked],
            }),
        }
    }
    if !blank {
        for bucket in &mut buckets {
            bucket.items.sort_by_key(Ranked::rank);
        }
        buckets.sort_by_key(|bucket| bucket.items[0].rank());
    }
    let mut entries = Vec::new();
    for Bucket { group, items } in buckets {
        if let Some(group) = group {
            entries.push(Entry::Heading(group));
        }
        entries.extend(items.into_iter().map(|ranked| ranked.entry));
    }
    entries
}

/// The commands of one group, as they are ranked.
struct Bucket {
    group: Option<Arc<str>>,
    items: Vec<Ranked>,
}

struct Ranked {
    score: i32,
    order: usize,
    entry: Entry,
}

impl Ranked {
    /// Best score first, then the order the commands were given in.
    fn rank(&self) -> (std::cmp::Reverse<i32>, usize) {
        (std::cmp::Reverse(self.score), self.order)
    }
}

/// How well `query` finds `command`, and what of its name to mark. Its name counts in full; a keyword, or its group's name before its own, counts for a little less and marks nothing it did not match in the name.
fn score(command: &Command, query: &str) -> Option<(i32, Vec<Range<usize>>)> {
    let direct = fuzzy_match(query, &command.label);
    let indirect = command
        .keywords
        .iter()
        .filter_map(|keyword| fuzzy_match(query, keyword))
        .chain(
            command
                .group
                .as_ref()
                .and_then(|group| fuzzy_match(query, &format!("{group} {}", command.label))),
        )
        .map(|found| found.score - INDIRECT_PENALTY)
        .max();
    let best = match (&direct, indirect) {
        (Some(found), Some(other)) => found.score.max(other),
        (Some(found), None) => found.score,
        (None, Some(other)) => other,
        (None, None) => return None,
    };
    Some((best, direct.map(|found| found.ranges).unwrap_or_default()))
}

struct Palette {
    results: Memo<Results>,
    /// The command the keyboard is on, by id. `None`, or an id no longer listed, means the first one it can stop on.
    cursor: RwSignal<Option<Arc<str>>>,
    /// Every row, a heading's too, at one height while the palette is open: nothing in a row runs onto a second line, so there is nothing to measure.
    places: RowPlaces,
    built: BuiltRows<Arc<str>>,
    run: Rc<dyn Fn(&Command)>,
    color: Reactive<Color>,
}

impl Palette {
    fn entries(&self) -> Rc<Vec<Rc<Entry>>> {
        self.results.with(|results| results.0.clone())
    }

    fn cursor_index(&self, entries: &[Rc<Entry>]) -> Option<usize> {
        let wanted = self.cursor.get();
        wanted
            .and_then(|id| {
                entries
                    .iter()
                    .position(|entry| entry.reachable_id() == Some(&id))
            })
            .or_else(|| {
                entries
                    .iter()
                    .position(|entry| entry.reachable_id().is_some())
            })
    }

    fn is_cursor(&self, id: &str) -> bool {
        let entries = self.entries();
        self.cursor_index(&entries)
            .and_then(|i| entries[i].reachable_id().cloned())
            .is_some_and(|at| &*at == id)
    }

    fn cursor_row(&self) -> Option<FocusId> {
        let entries = self.entries();
        let id = entries[self.cursor_index(&entries)?]
            .reachable_id()?
            .clone();
        self.built.get(&id)
    }

    /// Moves the cursor `delta` reachable rows, wrapping at the ends, and scrolls it into view.
    fn step(&self, delta: i64) {
        let entries = self.entries();
        let n = entries.len() as i64;
        if n == 0 {
            return;
        }
        let start = self.cursor_index(&entries).map_or(-1, |i| i as i64);
        let found = (1..=n).find_map(|k| {
            let i = (start + delta.signum() * k).rem_euclid(n) as usize;
            entries[i].reachable_id().map(|id| (i, id.clone()))
        });
        if let Some((i, id)) = found {
            self.cursor.set(Some(id));
            self.places.reveal(i);
        }
    }

    fn run_cursor(&self) {
        let entries = self.entries();
        if let Some(i) = self.cursor_index(&entries)
            && let Entry::Item { command, .. } = &*entries[i]
        {
            (self.run)(command);
        }
    }

    fn on_key(&self, key: &Key, modifiers: ModifiersState, dismiss: &dyn Fn()) -> bool {
        let Key::Named(named) = key else {
            return false;
        };
        let plain = !modifiers.is_ctrl && !modifiers.is_meta && !modifiers.is_alt;
        match named {
            NamedKey::ArrowDown if plain => self.step(1),
            NamedKey::ArrowUp if plain => self.step(-1),
            NamedKey::Enter => self.run_cursor(),
            NamedKey::Escape => dismiss(),
            _ => return false,
        }
        true
    }
}

fn build_open_palette(
    parts: Parts,
    open: Option<RwSignal<bool>>,
    dismiss: scrim::DismissFn,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Parts {
        commands,
        query,
        placeholder,
        label,
        empty_label,
        on_run,
        width,
        color,
        style,
    } = parts;
    let tokens = CommandPaletteTokens::current();
    let width = if width > 0.0 { width } else { tokens.width() };
    let row_height = ListTokens::current().row_height();
    let results = memo(move || {
        let commands = commands.get();
        let ranked = query.with(|query| rank(&commands, query));
        Results(Rc::new(ranked.into_iter().map(Rc::new).collect()))
    });
    let cursor = signal(None::<Arc<str>>);

    let mut made: Option<Rc<Palette>> = None;
    let run: Rc<dyn Fn(&Command)> = {
        let dismiss = dismiss.clone();
        Rc::new(move |command: &Command| {
            if command.disabled {
                return;
            }
            dismiss();
            if let Some(on_run) = &on_run {
                on_run(command);
            }
        })
    };
    // Short of the room as well as of eight rows: the card shrinks to the window it is in, and this is what gives.
    let scroll = LayoutScrollArea::new_with(
        LayoutStyle::new()
            .max_height(VISIBLE_ROWS as f32 * row_height)
            .flex_shrink(1.0),
        |viewport| {
            let palette = Rc::new(Palette {
                results,
                cursor,
                places: RowPlaces::fixed(viewport, row_height),
                built: BuiltRows::new(),
                run,
                color,
            });
            made = Some(palette.clone());
            let source = {
                let palette = palette.clone();
                move || palette.entries().as_ref().clone()
            };
            let rows = palette.places.clone().list(
                OVERSCAN,
                source,
                |entry: &Rc<Entry>| (**entry).clone(),
                move |_, entry| build_entry(&palette, &entry),
            )?;
            Ok(box_item(rows))
        },
    )?;
    let palette = made.expect("a scroll area builds its content before it returns");
    let scroll = {
        let keys = palette.clone();
        let cursor = palette.clone();
        let dismiss = dismiss.clone();
        scroll
            .control(Role::ListBox)
            .focus_style(|_r| RectStyle::default())
            .on_focused_key(move |key: &Key| keys.on_key(key, telar::modifiers(), &*dismiss))
            .active_descendant(move || cursor.cursor_row())
            .a11y_label(|| strings::text(strings::COMMANDS))
    };
    let list = scroll.focus_id();

    {
        let palette = palette.clone();
        let first = std::cell::Cell::new(true);
        effect(move || {
            palette.results.with(|_| ());
            if first.replace(false) {
                return;
            }
            palette.cursor.set(None);
            let viewport = palette.places.viewport();
            let (x, _) = viewport.peek_offset();
            viewport.scroll_to(x, 0.0);
        });
    }
    if let Some(open) = open {
        effect(move || {
            if !open.get() {
                query.set(String::new());
            }
        });
    }

    let field_width = width - card_pad() * 2.0;
    let keys = {
        let palette = palette.clone();
        let dismiss = dismiss.clone();
        Rc::new(move |key: &Key, modifiers: ModifiersState| {
            palette.on_key(key, modifiers, &*dismiss)
        })
    };
    let pointed = {
        let palette = palette.clone();
        Rc::new(move || palette.cursor_row())
    };
    let field = text_field(
        TextFieldProps::props()
            .value(query)
            .placeholder(Reactive::of(move || match &placeholder {
                Some(placeholder) => placeholder.get(),
                None => strings::text(strings::SEARCH_COMMANDS),
            }))
            .width(field_width)
            .on_key(keys)
            .active_descendant(pointed)
            .controls(Rc::new(move || list))
            .build(),
        Children::default(),
    )?;

    let empty = ReactiveList::new(
        move || vec![results.with(|results| results.0.is_empty())],
        |empty: &bool| *empty,
        move |empty| empty_state(empty, empty_label.clone()),
        0.0,
    )?;

    let room = scrim::Room::new();
    let card_box = move || {
        LayoutStyle::column()
            .width(width.min(room.inside(card_pad()).width))
            .padding_all(card_pad())
            .gap(card_pad())
            .min_height(0.0)
    };
    let name = label.clone();
    let card = StyledContainer::new(
        card_box(),
        move |_r| {
            let card = RectStyle::default()
                .with_fill(shared::surface())
                .with_border(Border::uniform(shared::border(), shared::border_width()))
                .with_radius(BorderRadius::all(CommandPaletteTokens::current().radius()));
            amend_surface(card, &style)
        },
        vec![field, box_item(scroll), box_item(empty)],
    )?
    .styled_by(card_box)
    .role(Role::Dialog)
    .a11y_label(move || match &name {
        Some(name) => name.get(),
        None => strings::text(strings::COMMAND_PALETTE),
    })
    .on_press(|| {});

    let backdrop_box = || {
        LayoutStyle::column()
            .flex_grow(1.0)
            .min_height(0.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::START)
            .padding_top(CommandPaletteTokens::current().inset_top())
            .padding_bottom(card_pad())
            .padding_horizontal(card_pad())
    };
    let backdrop = StyledContainer::new(
        backdrop_box(),
        |_r| RectStyle::default().with_fill(shared::overlay()),
        children![card],
    )?
    .styled_by(backdrop_box);
    room.follow(backdrop.layout_node());
    let backdrop = shared::dismissed_by_press(backdrop, move || dismiss());
    Ok(box_item(backdrop))
}

fn empty_state(
    empty: bool,
    empty_label: Option<Reactive<String>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if !empty {
        return Ok(box_item(Container::new(LayoutStyle::new(), vec![])?));
    }
    let text = Text::declaring(
        move || match &empty_label {
            Some(label) => label.get(),
            None => strings::text(strings::NO_RESULTS),
        },
        LayoutStyle::new(),
        |t| shared::quiet(t, 1.0),
    )?;
    let status = Container::new(
        LayoutStyle::row()
            .justify_content(JustifyContent::CENTER)
            .padding_vertical(card_pad()),
        children![text],
    )?
    .role(Role::Status);
    Ok(box_item(status))
}

fn build_entry(palette: &Rc<Palette>, entry: &Entry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    match entry {
        Entry::Heading(group) => heading(group.clone(), &palette.places),
        Entry::Item { command, marks } => build_row(palette, command, marks),
    }
}

fn heading(group: Arc<str>, places: &RowPlaces) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let heading = crate::list::group(
        crate::list::GroupProps::props()
            .label(Reactive::of(move || group.to_string()))
            .build(),
        Children::default(),
    )?;
    let slot = Container::new(
        places.sized(
            LayoutStyle::column()
                .flex_shrink(0.0)
                .justify_content(JustifyContent::END),
        ),
        vec![heading],
    )?;
    Ok(box_item(slot))
}

fn build_row(
    palette: &Rc<Palette>,
    command: &Command,
    marks: &[Range<usize>],
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = command.label.clone();
    let spans = {
        let marks: Vec<Range<usize>> = marks.to_vec();
        let color = palette.color.clone();
        move || {
            let accent = shared::resolve(&color, shared::accent);
            marks
                .iter()
                .map(|mark| {
                    Span::new(
                        mark.start as u32..mark.end as u32,
                        Declared::default()
                            .with_color(accent)
                            .with_font_weight(shared::strong_weight()),
                    )
                })
                .collect()
        }
    };
    let text = Text::spanned_declaring(
        move || label.to_string(),
        spans,
        LayoutStyle::new()
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .min_width(0.0),
        |t| shared::control_text(t, 1.0).with_text_wrap(TextWrap::NoWrap),
    )?;
    let mut content: Vec<Box<dyn LayoutItem>> =
        children![ClippedItem::new(box_item(text), Clip::both())];
    if let Some(chord) = command.shortcut.clone() {
        content.push(kbd(
            KbdProps::props()
                .chord(Reactive::of(move || chord.to_string()))
                .build(),
            Children::default(),
        )?);
    }

    let chord = command.shortcut.clone();
    let name = match &command.shortcut {
        Some(chord) => format!("{}, {}", command.label, shown_chord(chord)),
        None => command.label.to_string(),
    };
    let id = command.id.clone();
    // The cursor is the option a reader hears as selected, so it wears the selection's wash; the pointer moves the cursor, so a hovered row wears it too.
    let paint = |hovered: bool| {
        let (palette, id) = (palette.clone(), id.clone());
        move |_r| {
            let lit = hovered || palette.is_cursor(&id);
            row_paint(lit.then(|| tinted(&palette.color, shared::held_wash())))
        }
    };
    let disabled = command.disabled;
    let press = {
        let (palette, command) = (palette.clone(), command.clone());
        move || (palette.run)(&command)
    };
    let hover = {
        let (palette, id) = (palette.clone(), id.clone());
        move |inside: bool| {
            if inside && !disabled {
                palette.cursor.set(Some(id.clone()));
            }
        }
    };
    let places = palette.places.clone();
    let row_box = move || {
        places.sized(
            LayoutStyle::row()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::SPACE_BETWEEN)
                .flex_shrink(0.0)
                .padding_horizontal(ListTokens::current().pad_x())
                .gap(shared::gap_md()),
        )
    };
    let selected = {
        let (palette, id) = (palette.clone(), id.clone());
        move || palette.is_cursor(&id)
    };
    // Selected while the cursor is on it, as the list autocomplete pattern has it: nothing is chosen until it is run, and the cursor is what Enter runs.
    let row = StyledContainer::new(row_box(), paint(false), content)?
        .styled_by(row_box)
        .hover_style(paint(true))
        .disabled(move || disabled)
        .presented(Role::Option)
        .toggled(selected)
        .a11y_label(move || name.clone())
        .keyboard_shortcut(move || chord.as_deref().unwrap_or_default().to_string())
        .on_hover(hover)
        .on_press(press);
    if let Some(focus_id) = row.focus_id() {
        palette.built.track(id, focus_id);
    }
    Ok(box_item(row))
}

#[cfg(test)]
#[path = "command_palette_test.rs"]
mod tests;
