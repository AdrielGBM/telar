//! The caches and GPU resources every surface draws from, held once per thread rather than once per renderer.
//!
//! The same rule the CPU backend follows: what is addressed by *content* — a glyph, a tessellated path, a blurred
//! shadow — answers the same question for every surface, so one copy serves all of them. What is addressed by
//! *surface* — the swapchain, the per-frame instance buffers, the render pipelines that bake in a surface format
//! and a sample count — stays on the renderer.
//!
//! The GPU backend never got this treatment when the CPU one did, and the glyph atlas is where it showed: a
//! `TextPipeline` per renderer meant a 2048×2048 RGBA atlas texture per renderer, **16 MiB of VRAM each**. A shell
//! with nine surfaces held nine of them for the same few hundred glyphs. Unlike the CPU-side plane — `mmap`'d zero
//! pages that cost nothing until written — a `wgpu::Texture` is a real allocation the moment it is created, and it
//! never appears in RSS, which is why every measurement taken from `/proc` missed it entirely.
//!
//! Sharing is safe because the device is: `SHARED_GPU` hands every renderer in the process the same `wgpu::Device`,
//! so a texture or bind group made by one is valid for all.
//!
//! Process-global behind a lock, where the CPU backend gets away with a thread-local, because the GPU backend does
//! not render where it was built: the runtime hands each surface a `telar-render` thread of its own, and a frame
//! arrives on a thread that never ran the constructor. A thread-local there is not a cache — it is a per-thread
//! copy, which is the duplication this module exists to remove, and reaching for one that was never initialised on
//! this thread is a panic.

use renderer_cache::{Cache, Policy};
use renderer_text::{ATLAS_SIZE, GlyphAtlas, TextShaper};
use wgpu::{Device, Queue};

use crate::primitives::path::PathTessCache;
use crate::renderer::shadow::ShadowCacheKey;

/// VRAM held by one resolved shadow texture, which is always `Rgba8Unorm`.
fn resolved_shadow_bytes(resolved: &(wgpu::Texture, wgpu::TextureView)) -> usize {
    (resolved.0.width() as usize)
        .saturating_mul(resolved.0.height() as usize)
        .saturating_mul(4)
}

/// The one glyph atlas texture, its layout and the bind group that binds it.
///
/// Every renderer holds clones of the layout and bind group — `wgpu`'s resources are `Arc` handles, so a clone is
/// another name for the same GPU object, not another copy of it — and builds its own render pipeline against that
/// layout, because a pipeline bakes in the surface format and sample count and those differ between surfaces.
pub(crate) struct SharedAtlas {
    texture: wgpu::Texture,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
}

impl SharedAtlas {
    fn new(device: &Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("telar-text-atlas-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("telar-text-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("telar-text-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telar-text-atlas-bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            texture,
            bind_group_layout,
            bind_group,
        }
    }

    /// Uploads the glyphs packed since the last call.
    ///
    /// Draining the atlas's dirty rects is correct precisely because the texture is shared: whichever renderer
    /// syncs first writes them into the one texture every other renderer samples. With a texture per renderer it
    /// would not have been — the first to sync would have taken the rects and left the rest without the glyph.
    pub(crate) fn sync(&self, queue: &Queue, atlas: &mut GlyphAtlas) {
        // Collect the dirty rects into a local vec so the mutable borrow from `drain_dirty_rects()` is released before we read `atlas.pixels`.
        let dirty: Vec<[u32; 4]> = atlas.drain_dirty_rects().collect();
        for [x, y, w, h] in dirty {
            if w == 0 || h == 0 {
                continue;
            }
            // Upload only the dirty sub-rectangle. By providing the source slice starting at the (x, y) pixel and using the FULL atlas row stride for bytes_per_row, wgpu reads `w` pixels per row for `h` rows starting at (x, y) — the correct sub-rect upload pattern.
            let offset = ((y as usize * ATLAS_SIZE as usize) + x as usize) * 4;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &atlas.pixels[offset..],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * ATLAS_SIZE),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

/// Uploaded image textures, their bind groups, and the layout and samplers those are built against.
///
/// The layout and samplers move here with the cache because a bind group is only usable with the layout it was
/// made from: sharing the entries without sharing what they were built against would hand one renderer a group
/// belonging to another's pipeline. Together they are one set, so a wallpaper decoded once is uploaded once
/// instead of once per surface — the same duplication the atlas had, a level down.
pub(crate) struct SharedImages {
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    sampler_nearest: wgpu::Sampler,
    sampler_linear: wgpu::Sampler,
    textures: Cache<(u64, renderer_core::ImageFilter), GpuImage>,
    /// Bind groups over textures the application owns, kept apart from `textures` because neither of that
    /// cache's rules holds here: it evicts by the bytes it is holding, and an app-owned texture costs it
    /// none, while its entries keep their texture alive by RAII, which is not ours to do. A bind group does
    /// keep the view it was built from alive, so an entry stays valid even if the application drops its
    /// handle — leaving a plain count as the only bound needed. A `None` entry remembers a handle this
    /// backend cannot read, so the refusal is decided and reported once rather than every frame.
    external: lru::LruCache<(u64, renderer_core::ImageFilter), Option<wgpu::BindGroup>>,
}

/// How many app-owned handles are remembered at once, drawable or not. A window shows one or two
/// viewports, not dozens; the cap exists so an application that mints a fresh id every frame leaks nothing.
const EXTERNAL_BIND_GROUPS: usize = 8;

pub(crate) struct GpuImage {
    // Keeps the GPU texture alive via RAII; held to ensure the texture is not dropped while the image is in use.
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// What one uploaded texture costs in VRAM. Every texture created here is `Rgba8Unorm`.
fn texture_bytes(image: &GpuImage) -> usize {
    (image.texture.width() as usize)
        .saturating_mul(image.texture.height() as usize)
        .saturating_mul(4)
}

impl SharedImages {
    fn new(device: &Device, policy: Policy) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("telar-image-texture-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        Self {
            bind_group_layout,
            sampler_nearest: sampler(device, wgpu::FilterMode::Nearest),
            sampler_linear: sampler(device, wgpu::FilterMode::Linear),
            textures: Cache::new(policy, texture_bytes),
            external: lru::LruCache::new(
                std::num::NonZeroUsize::new(EXTERNAL_BIND_GROUPS).expect("non-zero"),
            ),
        }
    }

    pub(crate) fn bind_group(
        &mut self,
        device: &Device,
        queue: &Queue,
        image: &std::sync::Arc<renderer_core::ImageData>,
        filter: renderer_core::ImageFilter,
    ) -> Option<wgpu::BindGroup> {
        let key = (image.id, filter);
        if let Some(handle) = image.external_texture() {
            if let Some(cached) = self.external.get(&key) {
                return cached.clone();
            }
            let bind_group = match handle.as_any().downcast_ref::<crate::gpu::AppTexture>() {
                Some(app) => Some(self.view_bind_group(device, &app.view, filter)),
                // Built elsewhere and handed to a backend that cannot read it — another renderer's handle, or a hand-rolled `ExternalTexture`. Drawing nothing is the honest outcome, but doing it quietly is not: the command is well-formed and the region simply stays empty.
                None => {
                    tracing::warn!(
                        image_id = image.id,
                        "external texture handle did not come from `telar::gpu::image`; nothing will be drawn for it"
                    );
                    None
                }
            };
            // The failure is cached too, which is what keeps the warning to once per handle instead of once per frame, and leaves the LRU's cap as the bound on both.
            self.external.put(key, bind_group.clone());
            return bind_group;
        }
        if let Some(cached) = self.textures.get(&key) {
            return Some(cached.bind_group.clone());
        }

        let gpu_image = self.upload(device, queue, image, filter);
        let bind_group = gpu_image.bind_group.clone();
        // The bind group borrows the texture `gpu_image` owns, so a value the budget refused would be dropped here and leave the group pointing at nothing. Ratcheting the budget up to fit keeps correctness from depending on it.
        self.textures
            .grow_to(texture_bytes(&gpu_image).saturating_mul(2));
        self.textures.insert(key, gpu_image);
        Some(bind_group)
    }

    fn view_bind_group(
        &self,
        device: &Device,
        view: &wgpu::TextureView,
        filter: renderer_core::ImageFilter,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telar-image-texture-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(match filter {
                        renderer_core::ImageFilter::Nearest => &self.sampler_nearest,
                        renderer_core::ImageFilter::Linear => &self.sampler_linear,
                    }),
                },
            ],
        })
    }

    fn upload(
        &self,
        device: &Device,
        queue: &Queue,
        image: &std::sync::Arc<renderer_core::ImageData>,
        filter: renderer_core::ImageFilter,
    ) -> GpuImage {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("telar-image-texture"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            image.pixels(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * image.width),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.view_bind_group(device, &view, filter);

        GpuImage {
            texture,
            bind_group,
        }
    }
}

fn sampler(device: &Device, filter: wgpu::FilterMode) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: None,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

pub(crate) struct SharedCaches {
    pub(crate) text_shaper: TextShaper,
    pub(crate) path_tess: PathTessCache,
    pub(crate) shadow_resolved: Cache<ShadowCacheKey, (wgpu::Texture, wgpu::TextureView)>,
    pub(crate) atlas: SharedAtlas,
    pub(crate) images: SharedImages,
}

impl SharedCaches {
    fn new(device: &Device, font: renderer_core::FontConfig) -> Self {
        Self {
            text_shaper: TextShaper::with_config(renderer_text::TextShaperConfig {
                font,
                ..Default::default()
            }),
            path_tess: PathTessCache::new(renderer_cache::limits::GPU_PATH_TESS),
            shadow_resolved: Cache::new(renderer_cache::limits::GPU_SHADOW, resolved_shadow_bytes),
            atlas: SharedAtlas::new(device),
            images: SharedImages::new(device, renderer_cache::limits::gpu_texture(0, 0)),
        }
    }

    pub(crate) fn stats(&self) -> Vec<renderer_cache::CacheStat> {
        let mut stats = self.text_shaper.cache_stats();
        stats.extend(self.path_tess.stats());
        stats.push(self.shadow_resolved.stat("gpu.shadow"));
        stats.push(self.images.textures.stat("gpu.texture"));
        stats
    }
}

static CACHES: std::sync::OnceLock<std::sync::Mutex<SharedCaches>> = std::sync::OnceLock::new();

/// Builds this thread's shared caches if no renderer has yet, and hands back the handles a renderer needs to draw
/// from the shared atlas: its layout, to build a pipeline against, and its bind group, to bind at draw time.
///
/// Handles rather than a borrow because the text pipeline is built on a spawned thread, alongside the others, and
/// a `RefCell` borrow cannot cross that. Both are `Arc`s inside, so the clones name the one atlas rather than
/// copying it.
pub(crate) fn atlas_handles(
    device: &Device,
    font: renderer_core::FontConfig,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    with_caches(|caches| {
        (
            caches.atlas.bind_group_layout.clone(),
            caches.atlas.bind_group.clone(),
        )
    })
    .unwrap_or_else(|| {
        let caches = SharedCaches::new(device, font);
        let handles = (
            caches.atlas.bind_group_layout.clone(),
            caches.atlas.bind_group.clone(),
        );
        let _ = CACHES.set(std::sync::Mutex::new(caches));
        // Another renderer may have won the race; take whichever set actually landed.
        with_caches(|caches| {
            (
                caches.atlas.bind_group_layout.clone(),
                caches.atlas.bind_group.clone(),
            )
        })
        .unwrap_or(handles)
    })
}

/// Runs `f` against the shared caches if they exist yet, `None` before the first renderer built them.
fn with_caches<R>(f: impl FnOnce(&mut SharedCaches) -> R) -> Option<R> {
    let caches = CACHES.get()?;
    let mut guard = caches
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Some(f(&mut guard))
}

/// Opens the shared caches for the duration of `f`.
///
/// A no-op before the first renderer has built them, which no frame path can reach — every renderer takes its atlas
/// handles at construction, and that is what builds the set. Returning rather than panicking keeps a stray caller
/// from taking the process down over a census.
pub(crate) fn with_shared<R>(f: impl FnOnce(&mut SharedCaches) -> R) -> Option<R> {
    with_caches(f)
}

#[cfg(test)]
mod send_probe {
    // Can the shared set cross threads at all? A global behind a Mutex needs `Send`.
    #[test]
    fn shared_caches_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<super::SharedCaches>();
    }
}
