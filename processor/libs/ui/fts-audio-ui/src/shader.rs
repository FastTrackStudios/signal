//! A WGSL fragment shader, composited into an effect's UI.
//!
//! Lives here rather than in any one effect because every effect wants the
//! same three things — a device, a texture, and a way to hand that texture to
//! whoever is compositing — and none of them wants to own the wgpu plumbing
//! twice. An effect supplies only its `fs_main`.
//!
//! # How a shader gets on screen at all
//!
//! Blitz paints a custom widget by asking it for an `anyrender::Scene` — a
//! command list, not GPU calls. The escape hatch for a widget that wants the
//! GPU itself is a round trip through the renderer:
//!
//! 1. [`Widget::can_create_surfaces`] hands over the renderer's own context.
//!    On the vello backends that downcasts to a `DeviceHandle`, which is a
//!    wgpu `Device` and `Queue`.
//! 2. The widget renders into a `wgpu::Texture` of its own.
//! 3. `try_register_custom_resource` takes that texture and answers with a
//!    `ResourceId`.
//! 4. The scene paints a rectangle with `Paint::Resource(id)`, and vello
//!    composites the texture in its own pass.
//!
//! # Why the fallback is not a consolation prize
//!
//! Every one of those steps can decline. `renderer_specific_context` answers
//! `None` on a backend with no device to give — the CPU rasteriser, the
//! headless image renderer. `try_register_custom_resource` answers
//! `UnsupportedResourceKind` on a renderer that takes no textures and
//! `NotActive` before the surface is up, which is genuinely the case for the
//! first frames after a window opens.
//!
//! So the choice is made **per frame**, not per build: ask, and paint vectors
//! if the answer is no. WebGPU in a browser takes the shader path; WebGL-class
//! and CPU backends take the other one; neither needs to know which it is.
//!
//! # The texture is registered once, not per frame
//!
//! Registering hands vello a texture it keeps. Doing that every frame at 40 Hz
//! would hand it 40 textures a second and never take one back — a leak that no
//! screenshot would show and the machine would notice in about a minute. The
//! texture is created on the first paint and kept until the panel's size
//! changes; only then is the old one unregistered and a new one made.

use std::sync::Arc;

use anyrender::{PaintScene, RenderContext, ResourceId, Scene};
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Fill, ImageBrush, ImageSampler};

/// What the shader is told about the panel it is drawing.
///
/// One buffer, updated per frame. `std140`-friendly by construction: four
/// 4-float rows, so there is no padding to get wrong.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    /// `[width, height, seconds, engine]`.
    pub frame: [f32; 4],
    /// `[rate_hz, depth, mix, engaged]`.
    pub params: [f32; 4],
    /// `[r, g, b, 1]`, 0..=1.
    pub color: [f32; 4],
}

/// The GPU side of a painted panel: a device, a pipeline, and a texture.
pub struct ShaderSurface {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The texture and the id vello knows it by, plus the size it was made
    /// for — a resize invalidates all three together.
    target: Option<Target>,
}

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    id: ResourceId,
    size: (u32, u32),
}

/// The wgpu device and queue inside whatever the renderer boxed up, or
/// `None` if it boxed something this build cannot read.
///
/// `RenderContext::renderer_specific_context` hands back a `Box<dyn Any>`, so
/// the type has to be guessed exactly — and there is more than one right
/// answer. `anyrender_vello` boxes `wgpu_context::DeviceHandle`, and the
/// `wgpu_context` major depends on which `anyrender_vello` the host linked:
/// 0.11 (nice-plug's baseview path, for the plugins) brings 0.6, 0.12 (the
/// Blitz desktop path) brings 0.7. To `Any` those are unrelated types, and
/// the same binary can contain both.
///
/// Guessing only one of them is the failure this function exists to prevent:
/// the downcast fails, the panel keeps its vector fallback, and nothing
/// anywhere reports an error — the picture is simply less good than it should
/// be, for months, which is exactly what happened.
///
/// `vello::util::DeviceHandle` is tried too: vello declares its own, and a
/// host that hands that one over is not wrong to.
fn device_and_queue(ctx: Box<dyn std::any::Any>) -> Option<(wgpu::Device, wgpu::Queue)> {
    let ctx = match ctx.downcast::<wgpu_context_07::DeviceHandle>() {
        Ok(h) => return Some((h.device.clone(), h.queue.clone())),
        Err(ctx) => ctx,
    };
    let ctx = match ctx.downcast::<wgpu_context_06::DeviceHandle>() {
        Ok(h) => return Some((h.device.clone(), h.queue.clone())),
        Err(ctx) => ctx,
    };
    match ctx.downcast::<vello::util::DeviceHandle>() {
        Ok(h) => Some((h.device.clone(), h.queue.clone())),
        Err(_) => None,
    }
}

impl ShaderSurface {
    /// Build a surface from whatever the renderer handed over, if it handed
    /// over a wgpu device at all.
    ///
    /// `source` is the WGSL: it must define `fs_main`, taking the uniforms at
    /// group 0 binding 0. The vertex stage is supplied here — every panel is a
    /// rectangle, and a shader that had to draw its own would be a shader with
    /// a chance to get it wrong.
    #[must_use]
    pub fn new(ctx: Box<dyn std::any::Any>, source: &str) -> Option<Self> {
        Self::with_uniform_size(ctx, source, std::mem::size_of::<Uniforms>() as u64)
    }

    /// Same, for a panel whose uniforms are bigger than [`Uniforms`] — a
    /// spectrum's worth of bins, a chain's worth of band positions.
    ///
    /// The size is fixed at creation because a uniform buffer is: growing one
    /// means a new buffer, a new bind group and a new pipeline, and a panel
    /// that needed that every frame would be rebuilding its GPU state instead
    /// of drawing.
    #[must_use]
    pub fn with_uniform_size(
        ctx: Box<dyn std::any::Any>,
        source: &str,
        uniform_size: u64,
    ) -> Option<Self> {
        // A renderer that declines and a surface that fails to build look
        // identical from the panel's side — both leave the panel with no
        // surface and a scene that is still a legal drawing. One line says
        // which, so the next person chasing a missing shader does not have to
        // guess at it from the picture.
        let Some((device, queue)) = device_and_queue(ctx) else {
            tracing::debug!(
                shader.surface = "declined",
                "the renderer handed out no wgpu device; the vector fallback is what will draw"
            );
            return None;
        };
        tracing::debug!(
            shader.surface = "built",
            shader.uniform_bytes = uniform_size
        );
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fx-shader"),
            source: wgpu::ShaderSource::Wgsl(compose(source).into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fx-shader-uniforms"),
            size: uniform_size.max(16),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fx-shader-bind-layout"),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fx-shader-bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fx-shader-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fx-shader-pipeline"),
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
                    // What `register_texture` declares the image to be.
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

        Some(Self {
            device,
            queue,
            pipeline,
            uniforms,
            bind_group,
            target: None,
        })
    }

    /// Render the shader and paint the result into `scene`.
    ///
    /// `false` when the renderer would not take the texture — the caller then
    /// paints vectors instead, which is the whole point of asking rather than
    /// assuming.
    pub fn draw(
        &mut self,
        ctx: &mut dyn RenderContext,
        scene: &mut Scene,
        w: u32,
        h: u32,
        uniforms: Uniforms,
    ) -> bool {
        self.draw_raw(ctx, scene, w, h, bytemuck::bytes_of(&uniforms))
    }

    /// Same, for a panel with its own uniform block. See
    /// [`with_uniform_size`](Self::with_uniform_size).
    pub fn draw_raw(
        &mut self,
        ctx: &mut dyn RenderContext,
        scene: &mut Scene,
        w: u32,
        h: u32,
        uniforms: &[u8],
    ) -> bool {
        if w == 0 || h == 0 {
            return false;
        }
        if !self.ensure_target(ctx, w, h) {
            return false;
        }
        let Some(target) = &self.target else {
            return false;
        };

        self.queue.write_buffer(&self.uniforms, 0, uniforms);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fx-shader-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fx-shader-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Transparent, not black: the panel behind this is the
                        // rig's own ground, and a shader that cleared to black
                        // would punch a hole in it.
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            // Three vertices, no buffer: the vertex stage makes its own
            // full-screen triangle from the index.
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));

        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            anyrender::Paint::Resource(ImageBrush {
                image: target.id,
                sampler: ImageSampler::default(),
            }),
            None,
            &Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
        );
        true
    }

    /// Make (or remake) the texture for this size, registering it once.
    fn ensure_target(&mut self, ctx: &mut dyn RenderContext, w: u32, h: u32) -> bool {
        if self.target.as_ref().is_some_and(|t| t.size == (w, h)) {
            return true;
        }
        // A resize replaces the texture, so the old id must go back — this is
        // the only place a registration is ever dropped, and the only place
        // one is ever made.
        if let Some(old) = self.target.take() {
            ctx.unregister_resource(old.id);
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fx-shader-target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // Rendered into by us, then handed to vello — which does not
            // sample it in place: it blits the registered texture into its own
            // image atlas, so COPY_SRC is as required as the other two. Leave
            // it off and the frame dies in the command encoder rather than at
            // registration, which reads as a vello bug and is not one.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let Ok(id) = ctx.try_register_custom_resource(Box::new(texture.clone())) else {
            // The renderer will not hold a texture — a CPU backend, or a
            // surface that is not up yet. Not an error; the caller draws
            // vectors.
            return false;
        };
        self.target = Some(Target {
            texture,
            view,
            id,
            size: (w, h),
        });
        true
    }
}

impl Drop for ShaderSurface {
    fn drop(&mut self) {
        // The id cannot be returned here — unregistering needs the render
        // context, which a `Drop` does not have. The texture itself is freed
        // with the widget; what vello keeps is a handle that dies with its
        // renderer. Recorded because the asymmetry is surprising.
        let _ = &self.target;
    }
}

/// The default uniform block, for a panel that is happy with [`Uniforms`].
///
/// Kept out of [`PRELUDE`] because a panel with a uniform block of its own
/// binds the same slot: two `@group(0) @binding(0) var<uniform> u` in one
/// module is a redefinition, and the shader does not compile. See
/// [`compose`].
pub const DEFAULT_UNIFORMS: &str = r#"
struct Uniforms {
    // width, height, seconds, engine
    frame: vec4<f32>,
    // rate_hz, depth, mix, engaged
    params: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
"#;

/// The half of every shader that is the same: a full-screen triangle and the
/// colour rules, so a panel's WGSL is only its `fs_main`.
pub const PRELUDE: &str = r#"
// A brighter version of a colour that is MORE of that colour, not less.
//
// The obvious way to say "hotter" is `mix(tint, white, k)`, and it is wrong:
// it desaturates. Two delay taps overlapping used to pile up into a white
// blob — at the exact moment the picture should have been saying "there is a
// lot of delay here", it stopped being blue and started being a highlight.
// The colour carries which effect you are reading, and the brightest parts
// are where you are looking.
//
// So the hue is kept and deepened while the value rises. `k` above 1 is
// allowed and overdrives; the tone map picks up the rolloff.
fn hotter(c: vec3<f32>, k: f32) -> vec3<f32> {
    let peak = max(max(c.r, c.g), c.b);
    if (peak <= 0.0001) { return c; }
    // The hue at full value.
    let hue = c / peak;
    // Deepen it as it brightens: the dominant channel pulls further ahead,
    // which is what a saturated colour getting brighter actually looks like.
    let deep = mix(hue, hue * hue, clamp(k, 0.0, 1.0) * 0.55);
    return deep * peak * (1.0 + k * 1.35);
}

// Reinhard on LUMINANCE rather than per channel.
//
// Per-channel Reinhard compresses each channel toward 1 independently, so
// anything bright converges on white no matter what colour it started as —
// the same desaturation as above, arriving through the back door after every
// panel had been careful about its hue.
//
// On luminance the ratios between channels survive, so a very bright blue
// stays blue. Channels can land above 1 and clip, which desaturates the
// extreme top only: that is the one place it reads as intensity rather than
// as a mistake.
fn tonemap(c: vec3<f32>) -> vec3<f32> {
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (l <= 0.0001) { return c; }
    return c * ((l / (1.0 + l)) / l);
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle covering the viewport — cheaper than two and with no seam
// down the diagonal where the halves meet.
@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var out: VsOut;
    let x = f32((idx << 1u) & 2u);
    let y = f32(idx & 2u);
    let uv = vec2<f32>(x, y) * 2.0;
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    out.pos = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}
"#;

#[cfg(feature = "probe")]
pub mod probe;

/// A panel's WGSL as the device actually sees it.
///
/// The vertex stage is always supplied. The default uniform block is supplied
/// only when the panel did not bring its own: a panel with a bigger block —
/// a spectrum's worth of bins, a chain's worth of band positions — declares
/// `@group(0) @binding(0)` itself, and adding ours on top of it is a
/// redefinition that fails to compile.
///
/// This is also what the shader tests must validate. Validating a panel's
/// WGSL on its own passes while the composed module does not compile, which
/// is how the binding collision survived: every panel silently fell back to
/// vectors and no test was looking at the text the device was given.
#[must_use]
pub fn compose(source: &str) -> String {
    if source.contains("@binding(0)") {
        format!("{PRELUDE}\n{source}")
    } else {
        format!("{DEFAULT_UNIFORMS}\n{PRELUDE}\n{source}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The uniform block is four-float rows all the way down, so what Rust
    /// writes and what WGSL reads cannot drift apart over padding.
    #[test]
    fn the_uniform_block_has_no_padding() {
        assert_eq!(std::mem::size_of::<Uniforms>(), 48);
        assert_eq!(std::mem::align_of::<Uniforms>(), 4);
    }

    /// A surface cannot be built from a context that is not a wgpu device —
    /// which is exactly what a CPU backend hands over, and must be a `None`
    /// rather than a panic.
    #[test]
    fn a_context_without_a_device_makes_no_surface() {
        let not_a_device: Box<dyn std::any::Any> = Box::new(42u32);
        assert!(ShaderSurface::new(not_a_device, "").is_none());
    }
}
