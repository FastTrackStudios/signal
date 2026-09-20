//! A WGSL fragment shader, composited into the rig's UI.
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
        let handle = ctx.downcast::<vello::util::DeviceHandle>().ok()?;
        let device = Arc::new(handle.device.clone());
        let queue = Arc::new(handle.queue.clone());

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fx-shader"),
            source: wgpu::ShaderSource::Wgsl(format!("{PRELUDE}\n{source}").into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fx-shader-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
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
        if w == 0 || h == 0 {
            return false;
        }
        if !self.ensure_target(ctx, w, h) {
            return false;
        }
        let Some(target) = &self.target else {
            return false;
        };

        self.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

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
            // Rendered into by us, sampled by vello.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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

/// The half of every shader that is the same: a full-screen triangle and the
/// uniforms, so a panel's WGSL is only its `fs_main`.
pub(crate) const PRELUDE: &str = r#"
struct Uniforms {
    // width, height, seconds, engine
    frame: vec4<f32>,
    // rate_hz, depth, mix, engaged
    params: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

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
