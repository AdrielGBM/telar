//! [`command_palette`]: a dialog that finds a command by typing part of its name and runs it, with the keyboard never leaving the search field.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Accessible, AlignItems, BorderRadius, Children, Clip, ClippedItem, Color, Container, Declared,
    JustifyContent, Key, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Memo,
    ModifiersState, NamedKey, Props, Reactive, ReactiveList, RectStyle, RwSignal, ScrollViewport,
    ShapeStyle, Span, StyledContainer, Text, TextWrap, VirtualList, box_item, effect,
    focus::{FocusId, Role},
    memo, on_cleanup, signal,
};

use crate::fuzzy::fuzzy_match;
use crate::kbd::spoken_chord;
use crate::scrim;
use crate::shared;
use crate::strings;
use crate::text_field::{TextFieldProps, text_field};
use crate::{KbdProps, kbd};

const DEFAULT_WIDTH: f32 = 560.0;
/// How many rows show before the list scrolls.
const VISIBLE_ROWS: usize = 8;
const OVERSCAN: usize = 4;
/// What a match against a keyword or the group's name gives up to one against the command's own name.
const INDIRECT_PENALTY: i32 = 6;

fn row_height() -> f32 {
    crate::dropdown::ROW_HEIGHT
}
fn card_pad() -> f32 {
    shared::spacing()
}
fn top_offset() -> f32 {
    shared::spacing() * 12.0
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
    /// The chord shown beside it, written as [`kbd`](crate::kbd) takes it: `"Mod+K"`.
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
    /// Names the palette, so anything can open it with `open_overlay(id)` without holding its signal. Ignored when `open` is bound.
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
    /// The dialog's width in logical px. `0.0` (the default) means 560.
    #[props(default)]
    pub width: f32,
    /// Accent for the matched characters and the cursor. `Color::TRANSPARENT` (the default) falls back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
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
    } = props;
    let open = shared::resolve_open(open, id);
    let query = query.unwrap_or_else(|| signal(String::new()));
    let width = if width > 0.0 { width } else { DEFAULT_WIDTH };
    let parts = Parts {
        commands,
        query,
        placeholder,
        label,
        empty_label,
        on_run,
        width,
        color,
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
    width: f32,
    color: Reactive<Color>,
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
    viewport: ScrollViewport,
    /// The presented control of each row that is built, by command id: what the field points a reader at.
    built: RwSignal<HashMap<Arc<str>, FocusId>>,
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
        self.built.with(|built| built.get(&id).copied())
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
            self.reveal(i);
        }
    }

    /// Scrolls the least distance that brings row `index` fully into view, from the fixed row height because the row may not be built.
    fn reveal(&self, index: usize) {
        let (x, offset) = self.viewport.peek_offset();
        let height = self.viewport.rect().peek().height;
        if height <= 0.0 {
            return;
        }
        let row = row_height();
        let top = index as f32 * row;
        let bottom = top + row;
        let target = if top < offset {
            top
        } else if bottom > offset + height {
            bottom - height
        } else {
            return;
        };
        self.viewport.scroll_to(x, target);
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

    fn row_built(&self, id: Arc<str>, row: FocusId) {
        self.built.update(|built| {
            built.insert(id, row);
        });
    }

    fn row_dropped(&self, id: &str, row: FocusId) {
        if self.built.is_alive() && self.built.peek_with(|built| built.get(id) == Some(&row)) {
            self.built.update(|built| {
                built.remove(id);
            });
        }
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
    } = parts;
    let results = memo(move || {
        let commands = commands.get();
        let ranked = query.with(|query| rank(&commands, query));
        Results(Rc::new(ranked.into_iter().map(Rc::new).collect()))
    });
    let cursor = signal(None::<Arc<str>>);

    let mut made: Option<Rc<Palette>> = None;
    let list_height = move || {
        let count = results.with(|results| results.0.len()).min(VISIBLE_ROWS);
        LayoutStyle::new()
            .flex_column()
            .height(count as f32 * row_height())
    };
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
    let scroll = LayoutScrollArea::new_with(
        LayoutStyle::new().flex_grow(1.0).min_height(0.0),
        |viewport| {
            let palette = Rc::new(Palette {
                results,
                cursor,
                viewport,
                built: signal(HashMap::new()),
                run,
                color,
            });
            made = Some(palette.clone());
            let source = {
                let palette = palette.clone();
                move || palette.entries().as_ref().clone()
            };
            let rows = VirtualList::new(
                LayoutStyle::new().flex_column(),
                palette.viewport.clone(),
                row_height(),
                OVERSCAN,
                source,
                |entry: &Rc<Entry>| (**entry).clone(),
                move |_, entry| build_entry(&palette, &entry),
            )?;
            Ok(box_item(rows))
        },
    )?;
    let palette = made.expect("a scroll area builds its content before it returns");
    let scroll = Container::new(list_height(), vec![box_item(scroll)])?.styled_by(list_height);

    {
        let palette = palette.clone();
        let first = std::cell::Cell::new(true);
        effect(move || {
            palette.results.with(|_| ());
            if first.replace(false) {
                return;
            }
            palette.cursor.set(None);
            let (x, _) = palette.viewport.peek_offset();
            palette.viewport.scroll_to(x, 0.0);
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
            .build(),
        Children::default(),
    )?;

    let empty = ReactiveList::new(
        move || vec![results.with(|results| results.0.is_empty())],
        |empty: &bool| *empty,
        move |empty| empty_state(empty, empty_label.clone()),
        0.0,
    )?;

    let card_box = move || {
        LayoutStyle::new()
            .flex_column()
            .width(width)
            .padding_all(card_pad())
            .gap(card_pad())
    };
    let name = label.clone();
    let card = StyledContainer::new(
        card_box(),
        |_r| {
            RectStyle::default()
                .with_fill(shared::surface())
                .with_border(telar::Border::uniform(shared::border(), 1.0))
                .with_radius(BorderRadius::all(shared::radius() * 3.0))
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
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::START)
            .padding_top(top_offset())
            .padding_horizontal(card_pad())
    };
    let backdrop = StyledContainer::new(
        backdrop_box(),
        |_r| RectStyle::default().with_fill(scrim::SCRIM),
        vec![box_item(card)],
    )?
    .styled_by(backdrop_box)
    .on_press(move || dismiss());
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
    let status = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .justify_content(JustifyContent::CENTER)
            .padding_vertical(card_pad()),
        |_r| RectStyle::default(),
        vec![box_item(text)],
    )?
    .role(Role::Status);
    Ok(box_item(status))
}

fn build_entry(palette: &Rc<Palette>, entry: &Entry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    match entry {
        Entry::Heading(group) => heading(group.clone()),
        Entry::Item { command, marks } => build_row(palette, command, marks),
    }
}

fn heading(group: Arc<str>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let heading = crate::list::group(
        crate::list::GroupProps::props()
            .label(Reactive::of(move || group.to_string()))
            .build(),
        Children::default(),
    )?;
    let slot = Container::new(
        LayoutStyle::new()
            .flex_column()
            .height(row_height())
            .flex_shrink(0.0)
            .justify_content(JustifyContent::END),
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
                        Declared::default().with_color(accent).with_font_weight(600),
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
        vec![box_item(ClippedItem::new(box_item(text), Clip::both()))];
    if let Some(chord) = command.shortcut.clone() {
        content.push(kbd(
            KbdProps::props()
                .chord(Reactive::of(move || chord.to_string()))
                .build(),
            Children::default(),
        )?);
    }

    let name = match &command.shortcut {
        Some(chord) => format!("{}, {}", command.label, spoken_chord(chord)),
        None => command.label.to_string(),
    };
    let id = command.id.clone();
    let paint = |hovered: bool| {
        let (palette, id) = (palette.clone(), id.clone());
        move |_r| {
            if hovered || palette.is_cursor(&id) {
                crate::dropdown::option_row_hover_style(&palette.color)
            } else {
                RectStyle::default().with_radius(BorderRadius::all(shared::radius_sm()))
            }
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
    let row_box = || {
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .height(row_height())
            .flex_shrink(0.0)
            .padding_horizontal(10.0)
            .gap(8.0)
    };
    let row = StyledContainer::new(row_box(), paint(false), content)?
        .hover_style(paint(true))
        .disabled(move || disabled)
        .presented(Role::MenuItem)
        .a11y_label(move || name.clone())
        .on_hover(hover)
        .on_press(press);
    if let Some(focus_id) = row.focus_id() {
        palette.row_built(id.clone(), focus_id);
        let palette = palette.clone();
        on_cleanup(move || palette.row_dropped(&id, focus_id));
    }
    Ok(box_item(row))
}

#[cfg(test)]
#[path = "command_palette_test.rs"]
mod tests;
