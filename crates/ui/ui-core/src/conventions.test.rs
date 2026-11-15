use geometry_core::{LayoutGrid, Size};
use layout_core::LayoutStyle;
use platform_core::{
    Event, FocusBoundary, Key, KeyboardConvention, ModifiersState, SurfaceConventions,
};

use super::*;
use crate::{Container, LayoutItem, SurfaceCanvas, SurfaceEnv, track_layout};

fn cells() -> LayoutGrid {
    LayoutGrid::new(8.0, 16.0).expect("a positive, finite step")
}

fn canvas_on(grid: LayoutGrid) -> (SurfaceCanvas, layout_core::NodeId) {
    let env = SurfaceEnv {
        conventions: SurfaceConventions {
            grid,
            ..SurfaceConventions::default()
        },
        ..SurfaceEnv::default()
    };
    let probe = std::rc::Rc::new(std::cell::Cell::new(None));
    let seen = std::rc::Rc::clone(&probe);
    let canvas = SurfaceCanvas::new_in(Size::new(200.0, 100.0), &env, move || {
        let item = Container::new(LayoutStyle::new().width(20.0).height(20.0), vec![])?;
        seen.set(Some(item.layout_node()));
        let column = Container::new(LayoutStyle::column(), vec![Box::new(item)])?;
        Ok(Box::new(column) as Box<dyn LayoutItem>)
    })
    .unwrap();
    canvas.lay_out(Size::new(200.0, 100.0));
    (canvas, probe.get().expect("the probe was built"))
}

fn laid_out_size(canvas: &SurfaceCanvas, node: layout_core::NodeId) -> (f32, f32) {
    let rect = canvas.with_entered(|| track_layout(node).expect("a live node").peek());
    (rect.width, rect.height)
}

/// Each surface resolves its tree on its own grid, so a terminal and a window can share a thread and a process.
#[test]
fn two_surfaces_snap_the_same_style_to_their_own_grids() {
    let (terminal, on_cells) = canvas_on(cells());
    let (window, on_pixels) = canvas_on(LayoutGrid::UNIT);

    assert_eq!(laid_out_size(&terminal, on_cells), (24.0, 16.0));
    assert_eq!(laid_out_size(&window, on_pixels), (20.0, 20.0));
    assert_eq!(terminal.with_entered(surface_conventions).grid, cells());
    assert_eq!(
        window.with_entered(surface_conventions).grid,
        LayoutGrid::UNIT
    );
}

#[test]
fn a_surface_reads_back_the_conventions_it_was_put_on() {
    let canvas = SurfaceCanvas::new(Size::new(10.0, 10.0), || {
        Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?) as Box<dyn LayoutItem>)
    })
    .unwrap();
    let conventions = SurfaceConventions {
        grid: cells(),
        keyboard: KeyboardConvention::Mac,
        smooth_wheel: false,
        focus_boundary: FocusBoundary::Release,
    };
    canvas.with_entered(|| set_surface_conventions(conventions));
    assert_eq!(canvas.with_entered(surface_conventions), conventions);
    assert_eq!(surface_conventions(), SurfaceConventions::default());
}

fn canvas_with_keyboard(keyboard: KeyboardConvention) -> SurfaceCanvas {
    let env = SurfaceEnv {
        conventions: SurfaceConventions {
            keyboard,
            ..SurfaceConventions::default()
        },
        ..SurfaceEnv::default()
    };
    SurfaceCanvas::new_in(Size::new(10.0, 10.0), &env, || {
        Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?) as Box<dyn LayoutItem>)
    })
    .unwrap()
}

/// The same press is the primary modifier on the surface whose platform has a PC keyboard and not on the one whose platform has a Mac's, both on one thread.
#[test]
fn the_same_press_is_primary_on_one_surface_and_not_on_another() {
    let pc = canvas_with_keyboard(KeyboardConvention::Pc);
    let mac = canvas_with_keyboard(KeyboardConvention::Mac);
    let ctrl = ModifiersState {
        is_ctrl: true,
        ..ModifiersState::default()
    };
    let save = platform_core::Chord::parse("Mod+S").expect("a chord");
    let press = Event::ModifiersChanged { modifiers: ctrl };
    let primary_on = |canvas: &SurfaceCanvas| {
        canvas.with_entered(|| {
            crate::observe_keyboard(&press);
            (
                ctrl.is_primary(platform_core::keyboard_convention()),
                crate::chord_pressed(&save, &Key::Char('s')),
            )
        })
    };
    assert_eq!(primary_on(&pc), (true, true));
    assert_eq!(primary_on(&mac), (false, false));
}
