use std::rc::Rc;

use telar::preview::{
    Layout, Matrix, Play, PlayResult, PreviewEntry, Query, by_role, by_text, preview,
};
use telar::{
    AreaLayout, Axis, Container, DragSession, DropArea, DropPlace, Key, LayoutError, LayoutItem,
    NamedKey,
    LayoutStyle, Reactive, RectStyle, Role, RwSignal, StyledContainer, box_item, signal,
    track_layout,
};

use super::sample;
use crate::layout::reorder_zones::reorder_zones::{
    DropPreview, ReorderGroup, ReorderZoneProps, Slot, apply_zone_move,
};

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
            let group = ReorderGroup::new(&DragSession::new(), |slot| slot)
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
            let group = ReorderGroup::new(&DragSession::new(), |slot| slot)
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
            let strip = ReorderGroup::new(&session, |slot: Slot| slot.index)
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
            let list = ReorderGroup::new(&DragSession::new(), |slot: Slot| slot)
                .on_drop(move |from, place| {
                    if let DropPlace::Between(to) = place {
                        pages.update(|pages| {
                            telar::apply_move(pages, from.index, to.index);
                        });
                    }
                })
                .on_activate(move |slot| opened.set(pages.with(|pages| pages.get(slot.index).copied())))
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
