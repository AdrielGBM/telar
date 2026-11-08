use wayland_client::protocol::wl_shm::Format;

use super::super::swapchain::ShmLayout;
use super::{PoolResize, ShmFile, frame_bytes, layout_for, pool_resize, shm_format};

// What this machine's Hyprland advertised when probed with `wayland-info`: `AB24` is there, so its surfaces are drawn in place.
#[test]
fn a_compositor_that_takes_rgba_bytes_is_drawn_into_in_place() {
    let advertised = [
        Format::Abgr2101010,
        Format::Xbgr2101010,
        Format::Argb2101010,
        Format::Xrgb2101010,
        Format::Abgr8888,
        Format::Xbgr8888,
        Format::Xrgb8888,
        Format::Argb8888,
    ];
    assert_eq!(layout_for(&advertised), ShmLayout::Rgba);
    assert_eq!(shm_format(ShmLayout::Rgba), Format::Abgr8888);
}

#[test]
fn a_compositor_offering_only_the_mandatory_formats_gets_converted_frames() {
    assert_eq!(
        layout_for(&[Format::Argb8888, Format::Xrgb8888]),
        ShmLayout::Argb
    );
    assert_eq!(
        layout_for(&[Format::Xbgr8888, Format::Argb8888, Format::Xrgb8888]),
        ShmLayout::Argb,
        "without alpha the bytes do not match what tiny-skia draws"
    );
    assert_eq!(layout_for(&[]), ShmLayout::Argb);
    assert_eq!(shm_format(ShmLayout::Argb), Format::Argb8888);
}

#[test]
fn a_4k_pool_is_exactly_the_frame() {
    let size = frame_bytes(3840, 2160).unwrap();
    assert_eq!(size, 3840 * 2160 * 4);
    assert_eq!(ShmFile::new(size).unwrap().size(), 3840 * 2160 * 4);
}

#[test]
fn a_frame_past_the_protocols_i32_is_refused() {
    assert!(frame_bytes(46_341, 46_341).is_err());
}

#[test]
fn a_pool_grows_in_place_and_is_recreated_only_below_half() {
    assert_eq!(pool_resize(100, 101), PoolResize::Grow(101));
    assert_eq!(pool_resize(100, 100), PoolResize::Keep);
    assert_eq!(pool_resize(100, 50), PoolResize::Keep);
    assert_eq!(pool_resize(100, 49), PoolResize::Recreate(49));
}

#[test]
fn growing_and_shrinking_never_leaves_the_pool_smaller_than_the_frame() {
    let sizes = [
        (640, 480),
        (3840, 2160),
        (1920, 1080),
        (2560, 1440),
        (2000, 1500),
        (64, 64),
        (5120, 2880),
        (1, 1),
    ];
    let mut file = ShmFile::new(frame_bytes(1, 1).unwrap()).unwrap();
    for (width, height) in sizes {
        let needed = frame_bytes(width, height).unwrap();
        let change = file.fit(needed).unwrap();
        match change {
            PoolResize::Grow(size) | PoolResize::Recreate(size) => {
                assert_eq!(size, needed);
                assert_eq!(file.size(), needed, "{width}x{height} after {change:?}");
            }
            PoolResize::Keep => assert!(file.size() >= needed && file.size() / 2 <= needed),
        }

        let count = width as usize * height as usize;
        let value = 0xFF00_0000 | count as u32;
        file.pixels_mut(count).fill(value);
        let pixels = file.pixels(count);
        assert_eq!(pixels.len(), count);
        assert_eq!(pixels[count - 1], value, "{width}x{height} reads back");
    }
}
