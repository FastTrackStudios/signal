//! Run a panel's WGSL on a real GPU, with no window and no renderer, and get
//! the pixels back.
//!
//! # Why this exists
//!
//! A shader panel has exactly one place it can be seen: the live window. The
//! headless image renderer hands out no wgpu device, so the screenshot tool
//! draws the vector fallback and can never show the WGSL — which meant the
//! only way to look at a shader was to run the app, get the effect into the
//! right state by hand, and photograph the screen. For a panel with one look
//! that is merely slow. For a panel with EIGHT — one per algorithm family —
//! it is not practical at all, and the families that are hard to reach by
//! hand are exactly the ones that would ship unlooked-at.
//!
//! This owns the whole path instead: a device, the panel's own WGSL through
//! [`compose`](super::compose), the panel's own uniform bytes, and a
//! readback. What comes out is what the panel draws, not an approximation.
//!
//! A development tool — contact sheets, and tests that assert two states
//! actually differ — deliberately not on the panel's path at runtime.

use super::compose;

/// A GPU that can run a panel's shader offscreen.
pub struct Probe {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

/// One rendered frame: tightly packed RGBA8, `width * height * 4` bytes.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    /// Mean alpha across the frame, 0..=1.
    ///
    /// The cheapest question worth asking of a shader you cannot look at:
    /// did it draw anything at all? A panel that renders nothing and a panel
    /// that fails to render are the same picture, and this separates them.
    #[must_use]
    pub fn coverage(&self) -> f32 {
        if self.rgba.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.rgba.chunks_exact(4).map(|p| u64::from(p[3])).sum();
        let count = (self.rgba.len() / 4) as f64;
        (sum as f64 / count / 255.0) as f32
    }

    /// How different two frames are, 0..=1 — mean absolute difference over
    /// every channel.
    ///
    /// What a per-algorithm picture has to be able to prove: that two
    /// machines do not draw the same thing.
    #[must_use]
    pub fn difference(&self, other: &Self) -> f32 {
        if self.rgba.len() != other.rgba.len() || self.rgba.is_empty() {
            return 1.0;
        }
        let sum: u64 = self
            .rgba
            .iter()
            .zip(&other.rgba)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        (sum as f64 / self.rgba.len() as f64 / 255.0) as f32
    }
}

impl Probe {
    /// Open a GPU. `None` where there is not one — CI without a device, a
    /// container with no ICD — which is a skip rather than a failure.
    #[must_use]
    pub fn open() -> Option<Self> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fx-shader-probe"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()?;
        Some(Self { device, queue })
    }

    /// Render `source` with `uniforms` at `width` x `height`.
    ///
    /// `source` is the panel's WGSL exactly as it ships; it goes through
    /// `compose` here for the same reason it does on the panel's own path —
    /// validating or rendering the fragment alone tests something the device
    /// is never given.
    ///
    /// # Panics
    ///
    /// On a shader that does not compile, which is the point: a contact sheet
    /// that silently rendered nothing is worse than no contact sheet.
    #[must_use]
    pub fn render(&self, source: &str, uniforms: &[u8], width: u32, height: u32) -> Frame {
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("probe-shader"),
                source: wgpu::ShaderSource::Wgsl(compose(source).into()),
            });

        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("probe-uniforms"),
            size: (uniforms.len() as u64).max(16),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buf, 0, uniforms);

        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("probe-bind-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("probe-bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("probe-layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("probe-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("probe-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // A readback buffer's rows are padded to 256 bytes; the copy has to
        // respect that and the unpacking has to undo it.
        let unpadded = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("probe-readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("probe-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("probe-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let _ = rx.recv();

        let mapped = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity((unpadded * height) as usize);
        for row in 0..height {
            let start = (row * padded) as usize;
            rgba.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        readback.unmap();

        Frame {
            width,
            height,
            rgba,
        }
    }
}
