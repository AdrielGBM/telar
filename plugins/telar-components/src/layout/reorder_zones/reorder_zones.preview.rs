use std::collections::BTreeSet;
use std::rc::Rc;

use telar::preview::{
    Layout, Matrix, Play, PlayError, PlayResult, PreviewEntry, Query, by_role, by_text, preview,
};
use telar::{
    Accessible, AlignItems, AreaLayout, Axis, Container, DragSession, DropArea, DropPlace,
    HeadingLevel, Key, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Link, NamedKey,
    Reactive, RectStyle, Role, RwSignal, StyledContainer, VirtualRows, anchor, box_item, memo,
    signal, track_layout,
};

use super::sample;
use crate::layout::reorder_zones::reorder_zones::{
    DropPreview, ReorderGroup, ReorderZoneProps, Slot, VirtualZoneProps, apply_zone_move,
};
use crate::{ButtonProps, LinkProps, button, link};

/// Drags what `from` finds to the point `down` of the way down what `to` finds, as a hand carries a row onto another.
fn drag_onto(canvas: &mut Play, from: Query, to: Query, down: f32) -> PlayResult {
    let start = canvas.find(&from)?.rect;
    let end = canvas.find(&to)?.rect;
    let dx = end.x + end.width / 2.0 - (start.x + start.width / 2.0);
    let dy = end.y + end.height * down - (start.y + start.height / 2.0);
    canvas.drag(from, dx, dy)
}

/// An entry of the folder preview: a folder with how many files it holds, or a file.
#[derive(Clone)]
enum Entry {
    Folder(&'static str, usize),
    File(&'static str),
}

impl Entry {
    fn name(&self) -> String {
        match self {
            Entry::Folder(name, held) => format!("{name} ({held})"),
            Entry::File(name) => name.to_string(),
        }
    }
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(reorder_zones, "Two zones", |p| {
            let zones = signal(vec![
                vec!["Design", "Review"],
                vec!["Build", "Test", "Ship"],
            ]);
            let group = ReorderGroup::new(&DragSession::new(), Some)
                .drop_preview(p.arg("drop_preview", DropPreview::Line))
                .on_drop(move |from, place| {
                    if let DropPlace::Between(to) = place {
                        zones.update(|zones| {
                            apply_zone_move(zones, from, to);
                        });
                    }
                });
            let gap = p.arg("gap", 8.0f32);
            let columns = (0..2)
                .map(|zone| {
                    group.zone(
                        ReorderZoneProps::props()
                            .zone(zone)
                            .count(Reactive::of(move || zones.get()[zone].len()))
                            .item(Rc::new(move |index| {
                                sample::tile(Reactive::of(move || {
                                    zones.with(|zones| {
                                        zones[zone]
                                            .get(index)
                                            .copied()
                                            .unwrap_or_default()
                                            .to_string()
                                    })
                                }))
                            }))
                            .gap(gap)
                            .min_extent(p.arg("min_extent", 24.0f32))
                            .build(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, LayoutError>(Container::new(LayoutStyle::row().gap(24.0), columns)?)
        })
        .props(<ReorderZoneProps as telar::preview::HasPropsSchema>::schema)
        .title("Layout/Reorder zones")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let item = |name| by_role(Role::ListItem).named(name);
            drag_onto(canvas, item("Design"), item("Test"), 0.25)?;
            sample::expect_names(
                canvas,
                &by_role(Role::ListItem),
                &["Review", "Build", "Design", "Test", "Ship"],
            )
        }),
        preview!(reorder_zones, "Into folders", |_p| {
            let entries = signal(vec![
                Entry::Folder("Drafts", 0),
                Entry::Folder("Inbox", 0),
                Entry::File("notes.txt"),
                Entry::File("todo.txt"),
            ]);
            let is_folder = move |slot: Slot| {
                entries.with(|entries| matches!(entries.get(slot.index), Some(Entry::Folder(..))))
            };
            let group = ReorderGroup::new(&DragSession::new(), Some)
                .accepts(move |from, place| match place {
                    DropPlace::Between(_) => true,
                    DropPlace::Inside(into) => is_folder(into) && !is_folder(*from),
                })
                .label(move |slot| entries.with(|entries| entries[slot.index].name()))
                .on_drop(move |from, place| {
                    entries.update(|entries| match place {
                        DropPlace::Between(to) => {
                            telar::apply_move(entries, from.index, to.index);
                        }
                        DropPlace::Inside(into) => {
                            if let Entry::Folder(_, held) = &mut entries[into.index] {
                                *held += 1;
                            }
                            entries.remove(from.index);
                        }
                    });
                });
            group.zone(
                ReorderZoneProps::props()
                    .count(Reactive::of(move || entries.get().len()))
                    .item(Rc::new(move |index| {
                        sample::tile(Reactive::of(move || {
                            entries.with(|entries| {
                                entries.get(index).map(Entry::name).unwrap_or_default()
                            })
                        }))
                    }))
                    .gap(8.0)
                    .build(),
            )
        })
        .props(<ReorderZoneProps as telar::preview::HasPropsSchema>::schema)
        .title("Layout/Reorder zones")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            let item = |name| by_role(Role::ListItem).named(name);
            drag_onto(canvas, item("todo.txt"), item("Drafts (0)"), 0.5)?;
            sample::expect_names(
                canvas,
                &by_role(Role::ListItem),
                &["Drafts (1)", "Inbox (0)", "notes.txt"],
            )
        }),
        preview!(reorder_zones, "Onto another widget", |_p| {
            let tags = signal(vec!["Draft", "Review", "Ship"]);
            let binned = signal(0usize);
            let session = DragSession::new();
            let strip = ReorderGroup::new(&session, |slot: Slot| Some(slot.index))
                .on_drop(move |from, place| {
                    if let DropPlace::Between(to) = place {
                        tags.update(|tags| {
                            telar::apply_move(tags, from, to.index);
                        });
                    }
                })
                .zone(
                    ReorderZoneProps::props()
                        .count(Reactive::of(move || tags.get().len()))
                        .item(Rc::new(move |index| {
                            sample::tile(Reactive::of(move || {
                                tags.with(|tags| {
                                    tags.get(index).copied().unwrap_or_default().to_string()
                                })
                            }))
                        }))
                        .row(true)
                        .gap(8.0)
                        .build(),
                )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().gap(24.0),
                vec![strip, bin(&session, tags, binned)?],
            )?)
        })
        .props(<ReorderZoneProps as telar::preview::HasPropsSchema>::schema)
        .title("Layout/Reorder zones")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            drag_onto(
                canvas,
                by_role(Role::ListItem).named("Review"),
                by_text("Binned: 0"),
                0.5,
            )?;
            sample::expect_names(canvas, &by_role(Role::ListItem), &["Draft", "Ship"])?;
            canvas.expect_text("Binned: 1")
        }),
        preview!(reorder_zones, "Opened on a click", |_p| {
            let pages = signal(vec!["Cover", "Contents", "Chapter one"]);
            let opened = signal(None::<&'static str>);
            let list = ReorderGroup::new(&DragSession::new(), Some)
                .on_drop(move |from, place| {
                    if let DropPlace::Between(to) = place {
                        pages.update(|pages| {
                            telar::apply_move(pages, from.index, to.index);
                        });
                    }
                })
                .on_activate(move |slot| {
                    opened.set(pages.with(|pages| pages.get(slot.index).copied()))
                })
                .zone(
                    ReorderZoneProps::props()
                        .count(Reactive::of(move || pages.get().len()))
                        .item(Rc::new(move |index| {
                            sample::tile(Reactive::of(move || {
                                pages.with(|pages| {
                                    pages.get(index).copied().unwrap_or_default().to_string()
                                })
                            }))
                        }))
                        .gap(8.0)
                        .build(),
                )?;
            let shown = sample::text_of(Reactive::of(move || match opened.get() {
                Some(page) => format!("Opened: {page}"),
                None => "Opened: none".to_string(),
            }))?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().gap(24.0),
                vec![list, shown],
            )?)
        })
        .props(<ReorderZoneProps as telar::preview::HasPropsSchema>::schema)
        .title("Layout/Reorder zones")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            let page = |name| by_role(Role::ListItem).named(name);
            drag_onto(canvas, page("Cover"), page("Chapter one"), 0.75)?;
            canvas.expect_text("Opened: none")?;
            canvas.click(page("Contents"))?;
            canvas.expect_text("Opened: Contents")?;
            canvas.tab()?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_text("Opened: Chapter one")?;
            sample::expect_names(
                canvas,
                &by_role(Role::ListItem),
                &["Contents", "Chapter one", "Cover"],
            )
        }),
        preview!(reorder_zones, "Sections of a long text", |_p| {
            let sections = signal(
                (0..SECTIONS)
                    .map(|section| (section * SENTENCES..(section + 1) * SENTENCES).collect())
                    .collect::<Vec<Vec<usize>>>(),
            );
            let folded = signal(BTreeSet::<usize>::new());
            let lines = memo(move || {
                sections.with(|sections| folded.with(|folded| lines_of(sections, folded)))
            });
            let line_at = move |slot: Slot| lines.with(|lines| lines.get(slot.index).copied());
            let group =
                ReorderGroup::new(&DragSession::new(), move |slot: Slot| match line_at(slot) {
                    Some(Line::Sentence(id)) => Some(id),
                    _ => None,
                })
                .accepts(move |_, place| match place {
                    DropPlace::Between(to) => to.index > 0,
                    DropPlace::Inside(onto) => matches!(line_at(onto), Some(Line::Heading(_))),
                })
                .label(move |slot| match line_at(slot) {
                    Some(Line::Sentence(id)) => format!("Sentence {id}"),
                    _ => String::new(),
                })
                .on_drop(move |id, place| {
                    let lines = lines.get();
                    sections.update(|sections| move_sentence(sections, &lines, id, place));
                });
            let scroll = LayoutScrollArea::new_with(
                LayoutStyle::column().flex_grow(1.0).min_height(0.0),
                move |viewport| {
                    let rows = VirtualRows::new(viewport, 44.0);
                    rows.page_anchors(move || {
                        lines.with(|lines| {
                            lines
                                .iter()
                                .filter_map(|line| match line {
                                    Line::Heading(section) => Some((*line, section_anchor(*section))),
                                    Line::Sentence(_) => None,
                                })
                                .collect()
                        })
                    });
                    let zone = group.virtual_zone(
                        VirtualZoneProps::props()
                            .items(Reactive::of(move || lines.get()))
                            .item(Rc::new(move |_, line| match line {
                                Line::Heading(section) => heading(section, folded),
                                Line::Sentence(id) => {
                                    sample::tile(Reactive::of(move || format!("Sentence {id}")))
                                }
                            }))
                            .rows(rows)
                            .build(),
                    )?;
                    Ok(box_item(Container::new(
                        LayoutStyle::column().gap(12.0).padding_all(16.0),
                        vec![
                            sample::text(
                                "100 sections of 30 sentences: only what the scroll shows is built",
                            )?,
                            link(
                                LinkProps::props()
                                    .label("Go to section 80")
                                    .to(anchor(&section_anchor(80)))
                                    .build(),
                            )?,
                            zone,
                        ],
                    )?))
                },
            )?
            .stop_label(|| "A long text");
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().flex_grow(1.0).min_height(0.0),
                vec![box_item(scroll)],
            )?)
        })
        .props(<VirtualZoneProps<Line> as telar::preview::HasPropsSchema>::schema)
        .title("Layout/Reorder zones")
        .layout(Layout::Fullscreen)
        .tags(&["stateful"])
        .play(|canvas| {
            let sentence = |id: usize| by_role(Role::ListItem).named(format!("Sentence {id}"));
            drag_onto(canvas, sentence(0), sentence(2), 0.75)?;
            expect_place(canvas, &sentence(0), 4)?;
            canvas.click(by_role(Role::Button).named("Fold section 0"))?;
            canvas.expect_absent(sentence(1))?;
            expect_place(canvas, &by_role(Role::ListItem).named("Section 1"), 2)?;
            let heading = |section: usize| {
                by_role(Role::Heading(HeadingLevel::Two)).named(format!("Section {section}"))
            };
            canvas.expect(heading(1))?;
            canvas.expect_absent(heading(80))?;
            canvas.click(by_role(Role::Link(Link::default())).named("Go to section 80"))?;
            canvas.expect(heading(80))
        }),
    ]
}

/// A box of another widget's making that takes a tag dropped onto it out of `tags`, counting it in `binned`.
fn bin(
    session: &DragSession<usize>,
    tags: RwSignal<Vec<&'static str>>,
    binned: RwSignal<usize>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let bin = StyledContainer::new(
        LayoutStyle::new().padding_all(16.0),
        |_| RectStyle::default(),
        vec![sample::text_of(Reactive::of(move || {
            format!("Binned: {}", binned.get())
        }))?],
    )?;
    let node = bin.layout_node();
    let rect = track_layout(node)
        .ok_or_else(|| LayoutError::Engine("the bin is not in the live tree".into()))?;
    session.area(
        DropArea::new(
            node,
            Axis::Vertical,
            |_, place| place == DropPlace::Inside(0),
            move |index: usize, _| {
                tags.update(|tags| {
                    tags.remove(index);
                });
                binned.update(|binned| *binned += 1);
            },
        )
        .layout(move || {
            let at = rect.peek();
            AreaLayout {
                bounds: at,
                items: vec![at],
            }
        }),
    );
    Ok(box_item(bin))
}

const SECTIONS: usize = 100;
const SENTENCES: usize = 30;

/// One line of the long text: a section's heading, or a sentence by its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Line {
    Heading(usize),
    Sentence(usize),
}

/// The text as one list: each section's heading, then its sentences unless it is folded.
fn lines_of(sections: &[Vec<usize>], folded: &BTreeSet<usize>) -> Vec<Line> {
    sections
        .iter()
        .enumerate()
        .flat_map(|(section, sentences)| {
            let shown = match folded.contains(&section) {
                true => &[][..],
                false => sentences.as_slice(),
            };
            std::iter::once(Line::Heading(section))
                .chain(shown.iter().map(|id| Line::Sentence(*id)))
        })
        .collect()
}

/// Moves sentence `id` to `place` among `lines`, the text as it was shown when it was let go: after the line before a gap, or at the end of a section dropped onto its heading.
fn move_sentence(sections: &mut [Vec<usize>], lines: &[Line], id: usize, place: DropPlace<Slot>) {
    let target = match place {
        DropPlace::Inside(onto) => match lines.get(onto.index) {
            Some(Line::Heading(section)) => Target::End(*section),
            _ => return,
        },
        DropPlace::Between(to) => match to.index.checked_sub(1).and_then(|at| lines.get(at)) {
            Some(Line::Heading(section)) => Target::Start(*section),
            Some(Line::Sentence(after)) if *after != id => Target::After(*after),
            _ => return,
        },
    };
    for sentences in sections.iter_mut() {
        sentences.retain(|at| *at != id);
    }
    match target {
        Target::Start(section) => sections[section].insert(0, id),
        Target::End(section) => sections[section].push(id),
        Target::After(after) => {
            for sentences in sections.iter_mut() {
                if let Some(at) = sentences.iter().position(|at| *at == after) {
                    sentences.insert(at + 1, id);
                    return;
                }
            }
        }
    }
}

enum Target {
    Start(usize),
    End(usize),
    After(usize),
}

/// A section's heading, with the button that folds its sentences away and back.
fn heading(
    section: usize,
    folded: RwSignal<BTreeSet<usize>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let is_folded = move || folded.with(|folded| folded.contains(&section));
    let fold = button(
        ButtonProps::props()
            .label(Reactive::of(move || match is_folded() {
                true => format!("Unfold section {section}"),
                false => format!("Fold section {section}"),
            }))
            .on_press(move || {
                folded.update(|folded| {
                    if !folded.remove(&section) {
                        folded.insert(section);
                    }
                })
            })
            .build(),
    )?;
    Ok(box_item(Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .gap(12.0)
            .padding_top(12.0)
            .padding_bottom(4.0),
        vec![
            box_item(
                Container::new(
                    LayoutStyle::new(),
                    vec![sample::text_of(Reactive::of(move || {
                        format!("Section {section}")
                    }))?],
                )?
                .a11y_role(Role::Heading(HeadingLevel::Two)),
            ),
            fold,
        ],
    )?))
}

/// The anchor a link to section `section` names.
fn section_anchor(section: usize) -> String {
    format!("section-{section}")
}

/// That the item `query` matches says it stands at `place` among all the items, counted from one.
fn expect_place(canvas: &mut Play, query: &Query, place: u32) -> PlayResult {
    let found = canvas.find(query)?.position.map(|at| at.position.get());
    match found == Some(place) {
        true => Ok(()),
        false => Err(PlayError::new(format!(
            "expected {query} at {place}, but it is at {found:?}"
        ))),
    }
}
