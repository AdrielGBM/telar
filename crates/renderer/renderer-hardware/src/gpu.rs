//! The GPU objects every surface in the process shares, lent to an application that wants to draw its
//! own content into Telar's frame.
//!
//! Telar creates them: it opens the window, picks the backend and negotiates the optional features, and
//! an application asks for what it is already drawing with. Only one direction exists on purpose —
//! two `wgpu::Device`s cannot exchange a texture, so a viewport or a game frame is shareable only by
//! living on Telar's device, and letting an application supply its own would be a second way to reach
//! the same place with an initialization order to get wrong.

pub use crate::renderer::SharedGpu;
use crate::renderer::open_shared_gpu;
/// The exact `wgpu` Telar is built against. Going through this re-export is what keeps an
/// application's handles type-compatible with Telar's; a separate `wgpu` dependency that resolves to
/// another major version compiles fine on both sides and then refuses to share a single texture.
pub use wgpu;

use crate::renderer::SHARED_GPU;
use renderer_core::{ExternalTexture, ImageData};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const UNSET: u8 = u8::MAX;
static POWER: AtomicU8 = AtomicU8::new(UNSET);

/// Which adapter to open the process's device on. Takes effect only before that device exists — the first
/// renderer, or the first [`open`], settles it for the process, and a later call warns and does nothing.
///
/// The default is [`wgpu::PowerPreference::LowPower`], which is the reverse of what a renderer usually asks
/// for and is deliberate. A user interface is a few thousand draw commands: the integrated adapter runs it
/// without noticing, and it is already awake because it is the one compositing the desktop. Asking for the
/// discrete adapter on a hybrid laptop buys nothing measurable and costs two things that are — every frame
/// crosses devices to reach the display, and a discrete GPU that has been allowed to idle makes the next
/// frame wait while it clocks back up, which reads as a window answering the first key press late. The
/// application whose frame is genuinely heavy — a 3D viewport, a shader playground — is the one that should
/// say so.
///
/// `WGPU_POWER_PREF=low|high|none` wins over both, so a machine that disagrees needs no rebuild.
pub fn prefer(power: wgpu::PowerPreference) {
    if SHARED_GPU.get().is_some() {
        tracing::warn!(
            "the GPU is already open, so this preference arrived too late to be honoured"
        );
        return;
    }
    POWER.store(power as u8, Ordering::Relaxed);
}

pub(crate) fn preference() -> wgpu::PowerPreference {
    if let Some(asked) = wgpu::PowerPreference::from_env() {
        return asked;
    }
    match POWER.load(Ordering::Relaxed) {
        0 => wgpu::PowerPreference::None,
        2 => wgpu::PowerPreference::HighPerformance,
        _ => wgpu::PowerPreference::LowPower,
    }
}

#[derive(Debug)]
pub(crate) struct AppTexture {
    pub(crate) view: wgpu::TextureView,
}

impl ExternalTexture for AppTexture {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Presents a texture the application owns as an image Telar can place in its frame.
///
/// Telar only ever samples it: the application keeps rendering into the texture on its own schedule
/// and Telar shows whatever is there when it composes a frame. That is the whole point — the two
/// cadences are independent, and Telar never learns what the picture is of.
///
/// The view must belong to the device Telar is drawing with (see [`shared`]) — one from another device is
/// a different universe and nothing here can bridge it — carry `TEXTURE_BINDING` usage, and have a
/// float-filterable format, since Telar samples it through a filtering sampler.
///
/// `Rgba8Unorm` is the one to reach for: the texels are sampled as they are, premultiplied alpha and no
/// colour-space conversion, exactly like an image Telar uploaded itself, so an `-Srgb` view is decoded on
/// sample and lands in the frame with its colours shifted. Note this asks less than
/// [`TextureUi`](https://docs.rs/telar/latest/telar/struct.TextureUi.html) does of *its* texture, and for
/// the plain reason that this one is a source Telar reads and that one is a target Telar draws into.
///
/// `id` addresses the *view*, not its contents, which are expected to change constantly. Keep it
/// stable for as long as the view is, and change it when the view is replaced — a resize, a format
/// change — so the bind group built against the old one is dropped.
pub fn image(view: wgpu::TextureView, id: u64, width: u32, height: u32) -> ImageData {
    ImageData::external(Arc::new(AppTexture { view }), id, width, height)
}

/// The GPU objects Telar is drawing with, or `None` before its first renderer exists.
///
/// **Not reliably available from [`App::mount`]**, which is the tempting reading and is wrong: the hardware
/// renderer is built on a thread of its own and the tree is mounted without waiting for it, so a mount that
/// asks here is racing a build it will almost always lose. `None` means *no device yet*, which is a different
/// thing from the software backend and looks identical from here.
///
/// An application that needs the device to build its tree wants [`open`], which brings it up and hands the
/// build in flight the very same objects.
///
/// [`App::mount`]: https://docs.rs/telar/latest/telar/trait.App.html#method.mount
pub fn shared() -> Option<SharedGpu> {
    SHARED_GPU.get().cloned()
}

/// The same objects [`shared`] hands out, bringing them up if no renderer has yet.
///
/// [`shared`] answers `None` until Telar has built a renderer, which is the honest answer for an
/// application waiting on the window it asked Telar to open. It is a wall for one that opens no window —
/// a texture has to come from this device to be shareable, and there is nothing to have created it. Same
/// device either way: whoever asks first opens it and every later renderer takes that one.
pub fn open() -> Result<SharedGpu, renderer_core::RendererError> {
    open_shared_gpu()
}
