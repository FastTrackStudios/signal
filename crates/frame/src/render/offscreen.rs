//! Headless GPU rendering: a vello scene → RGBA pixels.
//!
//! One [`Offscreen`] holds the device and renderer, so rendering many frames
//! (parity runs, surface snapshots, previews) pays GPU setup once.

use vello::wgpu;
use vello::{AaConfig, RenderParams, Renderer, RendererOptions};

pub struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    /// The effects stage, created on first use.
    fx: Option<crate::render::fx::Stage>,
    /// Bands the last [`Offscreen::render`] took.
    last_bands: u32,
}

/// What a render target is cleared to before vello draws: vello writes
/// every pixel of a frame it renders, and none when a buffer overflows, so
/// a row of nothing but this is an overflow. (No render leaves a whole row
/// this colour: almost transparent, but not black.)
pub const SENTINEL: [u8; 4] = [3, 1, 4, 1];

/// Rows per band multiple: vello's tile height, so bands split on tiles.
const BAND_ALIGN: u32 = 16;

/// The usages a [`render_checked`] target needs.
pub const CHECKED_TARGET_USAGES: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST)
    .union(wgpu::TextureUsages::STORAGE_BINDING)
    .union(wgpu::TextureUsages::TEXTURE_BINDING);

/// Whether `rgba` is entirely [`SENTINEL`] (vello drew nothing).
#[must_use]
pub fn is_untouched(rgba: &[u8]) -> bool {
    rgba.as_chunks::<4>().0.iter().all(|p| *p == SENTINEL)
}

/// Render `scene` into `target` (an `Rgba8Unorm` texture with
/// [`CHECKED_TARGET_USAGES`]) like `Renderer::render_to_texture`, but never
/// leave it blank.
///
/// vello's GPU buffers (per-tile commands, tiles, segments, blend spill)
/// have fixed sizes that vello 0.10 cannot grow, and a frame that overflows
/// any of them renders *nothing*: coarse rasterisation flags the failure
/// and the fine stage skips every tile, with no error returned. A 6000 px
/// page overflows the per-tile command buffer on its own. So the target is
/// cleared to [`SENTINEL`] first; if its top row is still all sentinel
/// after the render, the frame is rendered again in horizontal bands
/// (halved until each fits — the viewport-sized buffers shrink with the
/// band) copied into place.
///
/// Returns the number of bands drawn (1 when the frame fit whole).
///
/// # Errors
/// On GPU failures, and when the scene overflows even the thinnest band
/// (its path data alone is too large).
pub fn render_checked(
    renderer: &mut vello::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &vello::Scene,
    target: &wgpu::Texture,
    base: peniko::Color,
    antialiasing: AaConfig,
) -> Result<u32, String> {
    let (width, height) = (target.width(), target.height());
    if render_band(
        renderer,
        device,
        queue,
        scene,
        target,
        0,
        base,
        antialiasing,
    )? {
        return Ok(1);
    }
    let mut bands = 0;
    // Bands still to draw, (top, height), top first; halved on overflow.
    let half = |h: u32| (h / 2).div_ceil(BAND_ALIGN) * BAND_ALIGN;
    let mut todo = vec![(half(height), height - half(height)), (0, half(height))];
    while let Some((top, h)) = todo.pop() {
        let band = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-band"),
            size: wgpu::Extent3d {
                width,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target.format(),
            usage: CHECKED_TARGET_USAGES,
            view_formats: &[],
        });
        if render_band(
            renderer,
            device,
            queue,
            scene,
            &band,
            top,
            base,
            antialiasing,
        )? {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-band-copy"),
            });
            encoder.copy_texture_to_texture(
                band.as_image_copy(),
                wgpu::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: top, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([encoder.finish()]);
            bands += 1;
            continue;
        }
        let split = half(h);
        if split == 0 || split >= h {
            return Err(format!(
                "the scene overflows vello's GPU buffers even in a {width}×{h} band; simplify it or render it smaller"
            ));
        }
        todo.push((top + split, h - split));
        todo.push((top, split));
    }
    Ok(bands)
}

/// Render rows `top..` of `scene` into `target`; `false` when vello's
/// buffers overflowed (its first row came back untouched).
#[allow(clippy::too_many_arguments)]
fn render_band(
    renderer: &mut vello::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &vello::Scene,
    target: &wgpu::Texture,
    top: u32,
    base: peniko::Color,
    antialiasing: AaConfig,
) -> Result<bool, String> {
    let shifted;
    let scene = if top == 0 {
        scene
    } else {
        let mut s = vello::Scene::new();
        s.append(
            scene,
            Some(kurbo::Affine::translate((0.0, -f64::from(top)))),
        );
        shifted = s;
        &shifted
    };
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    clear_to_sentinel(device, queue, &view);
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            &view,
            &RenderParams {
                base_color: base,
                width: target.width(),
                height: target.height(),
                antialiasing_method: antialiasing,
            },
        )
        .map_err(|e| format!("render_to_texture failed: {e}"))?;
    Ok(!is_untouched(&read_rows(device, queue, target, 0, 1)?))
}

/// Fill `view` with [`SENTINEL`].
fn clear_to_sentinel(device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView) {
    let c = |v: u8| f64::from(v) / 255.0;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("frame-sentinel"),
    });
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("frame-sentinel"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: c(SENTINEL[0]),
                    g: c(SENTINEL[1]),
                    b: c(SENTINEL[2]),
                    a: c(SENTINEL[3]),
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
    queue.submit([encoder.finish()]);
}

/// Render `scene` into `target` as [`render_checked`] renders a frame
/// that fits, without waiting for the check: the target's top row is read
/// back behind the render, and [`PendingCheck::poll`] says later whether
/// vello overflowed — `render_checked` again then, banded.
///
/// For a host drawing every frame, where waiting on the GPU for one row
/// cost a surface's first frame 3–28 ms (the wait is for everything
/// queued ahead of it, not for the row).
///
/// # Errors
/// On GPU failures.
pub fn render_deferred(
    renderer: &mut vello::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &vello::Scene,
    target: &wgpu::Texture,
    base: peniko::Color,
    antialiasing: AaConfig,
) -> Result<PendingCheck, String> {
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    clear_to_sentinel(device, queue, &view);
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            &view,
            &RenderParams {
                base_color: base,
                width: target.width(),
                height: target.height(),
                antialiasing_method: antialiasing,
            },
        )
        .map_err(|e| format!("render_to_texture failed: {e}"))?;
    Ok(PendingCheck(Readback::start(device, queue, target, 0, 1)))
}

/// A render's overflow check, read back behind it: see [`render_deferred`].
pub struct PendingCheck(Readback);

impl PendingCheck {
    /// `None` while the GPU is still at it; then whether the render
    /// overflowed (drew nothing). Never waits.
    ///
    /// # Errors
    /// When the readback failed.
    pub fn poll(&self, device: &wgpu::Device) -> Option<Result<bool, String>> {
        let _ = device.poll(wgpu::PollType::Poll);
        self.0
            .try_take()
            .map(|rows| rows.map(|rgba| is_untouched(&rgba)))
    }
}

/// Rows of an `Rgba8Unorm` texture on their way back to the CPU.
struct Readback {
    buffer: wgpu::Buffer,
    mapped: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    unpadded: u32,
    padded: u32,
    rows: u32,
}

impl Readback {
    /// Copy rows `top..top + rows` of `texture` out and ask for them mapped.
    fn start(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        top: u32,
        rows: u32,
    ) -> Self {
        let width = texture.width();
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-offscreen-readback"),
            size: u64::from(padded) * u64::from(rows),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame-offscreen-copy"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: top, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(rows),
                },
            },
            wgpu::Extent3d {
                width,
                height: rows,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (tx, mapped) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |v| {
            let _ = tx.send(v);
        });
        Self {
            buffer,
            mapped,
            unpadded,
            padded,
            rows,
        }
    }

    /// The rows (straight RGBA8, no padding) once mapped; `None` before.
    fn try_take(&self) -> Option<Result<Vec<u8>, String>> {
        match self.mapped.try_recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Some(Err(format!("map_async failed: {e}"))),
            Err(std::sync::mpsc::TryRecvError::Empty) => return None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Some(Err("map_async channel disconnected".into()));
            }
        }
        let mapped = self.buffer.slice(..).get_mapped_range();
        let (unpadded, padded) = (self.unpadded as usize, self.padded as usize);
        let mut rgba = vec![0_u8; unpadded * self.rows as usize];
        for row in 0..self.rows as usize {
            rgba[row * unpadded..(row + 1) * unpadded]
                .copy_from_slice(&mapped[row * padded..row * padded + unpadded]);
        }
        drop(mapped);
        self.buffer.unmap();
        Some(Ok(rgba))
    }
}

/// Copy rows `top..top + rows` of an `Rgba8Unorm` texture back to the CPU
/// (straight RGBA8, no padding), waiting for them.
fn read_rows(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    top: u32,
    rows: u32,
) -> Result<Vec<u8>, String> {
    let readback = Readback::start(device, queue, texture, top, rows);
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match readback.try_take() {
            Some(rows) => return rows,
            None => std::thread::sleep(std::time::Duration::from_millis(1)),
        }
    }
}

/// Straight RGBA8 pixels, row-major, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Offscreen {
    /// # Errors
    /// When no GPU adapter/device is available.
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|e| format!("request_adapter failed: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("frame-offscreen"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::default(),
        }))
        .map_err(|e| format!("request_device failed: {e}"))?;
        let renderer = Renderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: [AaConfig::Area, AaConfig::Msaa8].into_iter().collect(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|e| format!("failed to create renderer: {e}"))?;
        Ok(Self {
            device,
            queue,
            renderer,
            fx: None,
            last_bands: 0,
        })
    }

    /// Render `scene` at `width × height` over `base`, splitting it into
    /// bands when it overflows vello's buffers (see [`render_checked`]).
    /// [`Self::last_bands`] tells how many bands the frame took.
    ///
    /// # Errors
    /// On GPU failures, and when the scene overflows vello's buffers even
    /// in the thinnest band (never a blank frame).
    pub fn render(
        &mut self,
        scene: &vello::Scene,
        width: u32,
        height: u32,
        base: peniko::Color,
    ) -> Result<Pixels, String> {
        let (width, height) = (width.clamp(1, 8192), height.clamp(1, 8192));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-offscreen-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: CHECKED_TARGET_USAGES,
            view_formats: &[],
        });
        self.last_bands = render_checked(
            &mut self.renderer,
            &self.device,
            &self.queue,
            scene,
            &texture,
            base,
            AaConfig::Msaa8,
        )?;
        self.read_back(&texture)
    }

    /// Render `scene` with a deferred check ([`render_deferred`]) and
    /// poll it until the GPU answers: whether it overflowed.
    #[cfg(test)]
    pub(crate) fn deferred_overflow(
        &mut self,
        scene: &vello::Scene,
        width: u32,
        height: u32,
    ) -> bool {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-offscreen-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: CHECKED_TARGET_USAGES,
            view_formats: &[],
        });
        let base = peniko::Color::from_rgba8(255, 255, 255, 255);
        let check = render_deferred(
            &mut self.renderer,
            &self.device,
            &self.queue,
            scene,
            &texture,
            base,
            AaConfig::Area,
        )
        .expect("render");
        loop {
            if let Some(answer) = check.poll(&self.device) {
                return answer.expect("readback");
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// How many bands the last [`Self::render`] drew (1 when the frame fit
    /// vello's buffers whole).
    #[must_use]
    pub const fn last_bands(&self) -> u32 {
        self.last_bands
    }

    /// Render a document through the effects stage ([`crate::render::fx`]), in
    /// `frame.mode`. Antialiasing is MSAA 8×, like [`Self::render`].
    ///
    /// # Errors
    /// On GPU failures.
    pub fn render_with_effects(
        &mut self,
        frame: &crate::render::fx::Frame<'_>,
    ) -> Result<Pixels, String> {
        let frame = crate::render::fx::Frame {
            antialiasing: AaConfig::Msaa8,
            ..*frame
        };
        let stage = self
            .fx
            .get_or_insert_with(|| crate::render::fx::Stage::new(&self.device));
        let texture = stage.render(&self.device, &self.queue, &mut self.renderer, &frame)?;
        self.read_back(&texture)
    }

    /// Copy an `Rgba8Unorm` texture back to the CPU.
    fn read_back(&self, texture: &wgpu::Texture) -> Result<Pixels, String> {
        let rgba = read_rows(&self.device, &self.queue, texture, 0, texture.height())?;
        Ok(Pixels {
            width: texture.width(),
            height: texture.height(),
            rgba,
        })
    }
}
