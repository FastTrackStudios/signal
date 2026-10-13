//! A headless Vello renderer that never reads the frame back.
//!
//! # Why not `anyrender`'s `ImageRenderer`
//!
//! Because it measures itself. `VelloImageRenderer::render_to_vec` ends
//! every frame with `copy_texture_to_buffer` — 29 MB of GPU->CPU traffic
//! at 5120x1440, which the windowed app never performs because it
//! presents. That was tolerable while a frame cost 9 ms and the copy was
//! half of it. Once culling brought the drawing itself to about 0.1 ms
//! the copy was fifty times the signal, its own run-to-run drift (4.5 ms
//! to 5.6 ms, measured) swamped everything, and subtracting a median
//! floor started manufacturing p99s that moved several milliseconds
//! between identical runs.
//!
//! You cannot subtract your way out of that. The instrument had to go.
//!
//! # What this measures instead
//!
//! Frames are rendered to a texture and submitted, `BATCH` of them
//! back-to-back, and the GPU is waited on exactly once at the end. No
//! frame is ever copied back. What comes out is THROUGHPUT — how fast
//! this machine can produce arrangement frames — which is the quantity a
//! target frame rate is actually about, and it is measured without a
//! per-frame synchronisation that would serialise a pipeline the real
//! app keeps full.
//!
//! It is still not the windowed number: there is no surface, no
//! compositor and no present here. Quote it as the ceiling on drawing.

use anyrender::PaintScene;
use anyrender_vello::VelloScenePainter;
use eyre::{Result, WrapErr, eyre};
use vello::wgpu;

/// How many frames are submitted between waits.
///
/// Large enough that one `poll` is amortised to nothing, small enough
/// that a phase still reports several independent batches rather than
/// one number.
pub const BATCH: usize = 30;

/// A wgpu device, a Vello renderer and a texture to draw into.
pub struct Headless {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello::Renderer,
    scene: vello::Scene,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl Headless {
    /// Open a device and size the target.
    ///
    /// # Errors
    ///
    /// When there is no usable GPU. Deliberately an error rather than a
    /// software fallback: a benchmark that quietly rasterised on the CPU
    /// would report a number for a machine nobody has, and it would
    /// report it in the same format as a real one.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| {
            eyre!("no wgpu adapter ({e}): this benchmark needs a real GPU to mean anything")
        })?;

        // Which device the numbers came off. A benchmark that quietly
        // fell back to a software rasteriser reports the same shape of
        // table as one that did not, and the difference is two orders of
        // magnitude — so the adapter is stated rather than assumed.
        let info = adapter.get_info();
        tracing::info!(
            gpu.name = %info.name,
            gpu.backend = ?info.backend,
            gpu.kind = ?info.device_type,
            "rendering on"
        );
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("session-daw headless"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .wrap_err("opening a wgpu device")?;

        let renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                num_init_threads: None,
                antialiasing_support: vello::AaSupport::area_only(),
                pipeline_cache: None,
            },
        )
        .map_err(|e| eyre!("creating the vello renderer: {e}"))?;

        let view = target(&device, width, height);
        Ok(Self {
            device,
            queue,
            renderer,
            scene: vello::Scene::new(),
            view,
            width,
            height,
        })
    }

    /// The device and queue, for a renderer of its own to share — a
    /// frame face rendering its effects stage on the same GPU.
    #[must_use]
    pub fn gpu(&self) -> (wgpu::Device, wgpu::Queue) {
        (self.device.clone(), self.queue.clone())
    }

    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Record and submit one frame. Does NOT wait — see [`Self::wait`].
    ///
    /// Returns how long the CPU spent painting, which is the part we
    /// write and the only part measurable without a fence.
    ///
    /// # Errors
    ///
    /// When Vello cannot encode the scene.
    pub fn frame<F>(&mut self, draw: F) -> Result<f64>
    where
        F: FnOnce(&mut VelloScenePainter<'_, '_>),
    {
        let started = web_time::Instant::now();
        // The image/window renderers hand their painter a renderer and
        // a device so it can register image resources. The arrangement
        // records solid fills and nothing else, so the plain constructor
        // is the whole of what is needed — and when glyphs arrive they
        // will need a real device handle here, which is worth failing
        // loudly over rather than silently dropping.
        let mut painter = VelloScenePainter::new(&mut self.scene);
        painter.reset();
        draw(&mut painter);
        let cpu_ms = started.elapsed().as_secs_f64() * 1000.0;

        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &self.view,
                &vello::RenderParams {
                    base_color: vello::peniko::Color::TRANSPARENT,
                    width: self.width,
                    height: self.height,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .map_err(|e| eyre!("rendering the arrangement: {e}"))?;
        self.scene.reset();
        Ok(cpu_ms)
    }

    /// Block until everything submitted has actually finished.
    ///
    /// Without this the loop would time how fast frames can be QUEUED,
    /// which on a pipelined GPU is a much prettier number and not one
    /// anybody can watch.
    ///
    /// # Errors
    ///
    /// When the device is lost or the wait times out.
    pub fn wait(&self) -> Result<()> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|e| eyre!("waiting for the gpu: {e}"))?;
        Ok(())
    }
}

fn target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("arrangement target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // Vello writes the target from a compute shader.
            usage: wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}
