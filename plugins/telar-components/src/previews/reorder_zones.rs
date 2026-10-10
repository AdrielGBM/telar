use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Container, Key, LayoutError, LayoutStyle, NamedKey, Reactive, Role, signal};

use super::sample;
use crate::reorder_zones::{ReorderGroup, ReorderZoneProps, apply_zone_move};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(reorder_zones, "Two zones", |p| {
            let zones = signal(vec![
                vec!["Design", "Review"],
                vec!["Build", "Test", "Ship"],
            ]);
            let group = ReorderGroup::new().on_move(move |from, to| {
                zones.update(|zones| {
                    apply_zone_move(zones, from, to);
                });
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
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_row().gap(24.0),
                columns,
            )?)
        })
        .props(<ReorderZoneProps as telar::preview::HasPropsSchema>::schema)
        .title("Layout/Reorder zones")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::ListItem).named("Design"))?;
            canvas.press(Key::Named(NamedKey::Space))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Space))?;
            sample::expect_names(
                canvas,
                &by_role(Role::ListItem),
                &["Review", "Build", "Design", "Test", "Ship"],
            )
        }),
    ]
}

