//! GPU pipelines of the effects stage and the fullscreen passes over them.

use std::collections::{HashMap, HashSet};

use vello::wgpu;
use wgpu::util::DeviceExt as _;

/// The frame's texture format (vello's target).
pub(super) const TARGET: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Blur intermediates: premultiplied light, with headroom.
pub(super) const LIGHT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// `Pass` in `util.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct PassUniforms {
    pub dir: [f32; 2],
    pub sigma: f32,
    pub taps: i32,
    pub step: u32,
    pub premul: u32,
    pub gain: f32,
    pub inside: f32,
}

impl PassUniforms {
    fn bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(32);
        for f in self.dir {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&self.sigma.to_le_bytes());
        b.extend_from_slice(&self.taps.to_le_bytes());
        b.extend_from_slice(&self.step.to_le_bytes());
        b.extend_from_slice(&self.premul.to_le_bytes());
        b.extend_from_slice(&self.gain.to_le_bytes());
        b.extend_from_slice(&self.inside.to_le_bytes());
        b
    }
}

/// `FxUniforms` in `prelude.wgsl` (288 bytes).
#[derive(Clone, Copy, Default)]
pub(super) struct FxUniforms {
    pub time_s: f32,
    pub value: f32,
    pub px_per_unit: f32,
    pub param_mask: u32,
    pub resolution: [f32; 2],
    pub node_size: [f32; 2],
    pub to_local: [[f32; 4]; 2],
    pub to_px: [[f32; 4]; 2],
    pub params: [f32; 16],
    /// `(offset, len)` into the data buffer per input slot, two per row.
    pub inputs: [[u32; 4]; 8],
}

impl FxUniforms {
    fn bytes(&self) -> Vec<u8> {
        let head = [self.time_s, self.value, self.px_per_unit];
        let tail = self
            .resolution
            .iter()
            .chain(&self.node_size)
            .chain(self.to_local.iter().chain(&self.to_px).flatten())
            .chain(&self.params);
        let mut b = Vec::with_capacity(288);
        head.iter()
            .for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
        b.extend_from_slice(&self.param_mask.to_le_bytes());
        tail.for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
        self.inputs
            .iter()
            .flatten()
            .for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
        debug_assert_eq!(b.len(), 288);
        b
    }
}

/// How a pass writes its target.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Blend {
    /// Replace.
    None,
    /// Premultiplied source over the target.
    Over,
    /// Add the source colour, keep the target alpha.
    Add,
}

impl Blend {
    fn state(self) -> Option<wgpu::BlendState> {
        match self {
            Self::None => None,
            Self::Over => Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            Self::Add => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
        }
    }
}

/// A compiled effect: its pipeline, the input slots its source reads, and
/// whether it generates (never reads its source); `None` = failed.
type Compiled = Option<(wgpu::RenderPipeline, Vec<String>, bool)>;

/// Clones share one cache of compiled effects: see `SharedPasses`.
#[derive(Clone)]
pub(super) struct Passes {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    blit: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    bloom: wgpu::RenderPipeline,
    /// Compiled effects by shader id, with the input slots their source
    /// reads; `None` = failed (drawn plainly).
    effects: HashMap<String, Option<(wgpu::RenderPipeline, Vec<String>)>>,
    /// Compiled effects that never read their source (`sample_src`,
    /// `src_tex`): generators, drawing their own picture.
    generators: HashSet<String>,
    /// Bound as the data buffer where a pass has no inputs.
    empty_data: wgpu::Buffer,
    /// Shader ids whose failure was already reported.
    reported: HashSet<String>,
    /// The effects compiled by this and every clone of it, by shader id.
    compiled: std::sync::Arc<std::sync::Mutex<HashMap<String, Compiled>>>,
}

impl Passes {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame-fx"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // A second texture (the bloom's emitter mask); other passes
                // bind their source again.
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Effect inputs (`fx_data`); other passes bind a stub.
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let empty_data = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("frame-fx-no-data"),
            contents: &[0; 16],
            usage: wgpu::BufferUsages::STORAGE,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("frame-fx"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("frame-fx-linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let util = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("frame-fx-util"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgsl/util.wgsl").into()),
        });
        let make = |fs: &str, format, blend: Blend| {
            pipeline(
                device,
                &pipeline_layout,
                &util,
                "vs_main",
                fs,
                format,
                blend,
            )
        };
        Self {
            blit: make("fs_blit", TARGET, Blend::Over),
            down: make("fs_down", LIGHT, Blend::None),
            blur: make("fs_blur", LIGHT, Blend::None),
            bloom: make("fs_bloom", TARGET, Blend::Add),
            layout,
            pipeline_layout,
            sampler,
            effects: HashMap::new(),
            generators: HashSet::new(),
            empty_data,
            reported: HashSet::new(),
            compiled: std::sync::Arc::default(),
        }
    }

    /// The compiled effect for `id` (compiling and caching it on first use);
    /// `None` when it has no source or fails to compile — reported once.
    pub fn effect(
        &mut self,
        device: &wgpu::Device,
        doc: &document_proto::Scene,
        id: &str,
    ) -> Option<&wgpu::RenderPipeline> {
        if !self.effects.contains_key(id) {
            let Some(body) = super::effect_source(doc, id) else {
                // Not cached: a missing asset may appear on a reload.
                self.report(
                    id,
                    &format!("no such shader (built-ins: {})", super::BUILTINS.join(", ")),
                );
                return None;
            };
            // Built-in and asset ids (content hashes) name fixed sources, so
            // the result — pipeline or failure — is cached for good, and
            // shared with the stages this one's passes are shared with.
            let known = self.compiled.lock().ok().and_then(|c| c.get(id).cloned());
            let entry = match known {
                Some(entry) => entry,
                None => {
                    let compiled =
                        compile(device, &self.pipeline_layout, &super::effect_module(&body));
                    let entry = compiled
                        .map_err(|e| self.report(id, &e))
                        .ok()
                        .map(|p| (p, super::effect_inputs(&body), super::generates(&body)));
                    if let Ok(mut c) = self.compiled.lock() {
                        c.insert(id.to_string(), entry.clone());
                    }
                    entry
                }
            };
            if entry.as_ref().is_some_and(|(_, _, generates)| *generates) {
                self.generators.insert(id.to_string());
            }
            self.effects
                .insert(id.to_string(), entry.map(|(p, slots, _)| (p, slots)));
        }
        self.cached_effect(id)
    }

    /// Whether `id` was compiled by this or a clone of it.
    #[cfg(test)]
    pub fn has_compiled(&self, id: &str) -> bool {
        self.compiled
            .lock()
            .is_ok_and(|c| c.get(id).is_some_and(Option::is_some))
    }

    /// The effect for `id` if it's already compiled.
    pub fn cached_effect(&self, id: &str) -> Option<&wgpu::RenderPipeline> {
        self.effects
            .get(id)
            .and_then(Option::as_ref)
            .map(|(p, _)| p)
    }

    /// Whether a compiled effect draws without reading its source.
    pub fn generates(&self, id: &str) -> bool {
        self.generators.contains(id)
    }

    /// The input slots (`IN_<NAME>`, in slot order) a compiled effect reads.
    pub fn effect_slots(&self, id: &str) -> &[String] {
        self.effects
            .get(id)
            .and_then(Option::as_ref)
            .map_or(&[], |(_, s)| s.as_slice())
    }

    fn report(&mut self, id: &str, error: &str) {
        if self.reported.insert(id.to_string()) {
            eprintln!("frame fx: shader `{id}` unavailable, drawing its subtree plainly: {error}");
        }
    }

    fn bind(
        &self,
        device: &wgpu::Device,
        uniforms: &[u8],
        src: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.bind2(device, uniforms, src, src, &self.empty_data)
    }

    fn bind2(
        &self,
        device: &wgpu::Device,
        uniforms: &[u8],
        src: &wgpu::TextureView,
        second: &wgpu::TextureView,
        data: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("frame-fx-uniforms"),
            contents: uniforms,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame-fx"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(second),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: data.as_entire_binding(),
                },
            ],
        })
    }

    fn run(
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        bind: &wgpu::BindGroup,
        dst: &wgpu::TextureView,
        clear: bool,
    ) {
        Self::run_in(encoder, pipeline, bind, dst, clear, None);
    }

    /// [`Self::run`], only inside `scissor` (`[x, y, w, h]`, target px).
    fn run_in(
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        bind: &wgpu::BindGroup,
        dst: &wgpu::TextureView,
        clear: bool,
        scissor: Option<[u32; 4]>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("frame-fx-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind, &[]);
        if let Some([x, y, w, h]) = scissor {
            pass.set_scissor_rect(x, y, w, h);
        }
        pass.draw(0..3, 0..1);
    }

    /// Straight-alpha `src` over `dst`, inside each of `rects` (`[x, y, w,
    /// h]`, target px).
    pub fn blit_over_in(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        rects: &[[u32; 4]],
    ) {
        let bind = self.bind(device, &PassUniforms::default().bytes(), src);
        for r in rects.iter().filter(|r| r[2] > 0 && r[3] > 0) {
            Self::run_in(encoder, &self.blit, &bind, dst, false, Some(*r));
        }
    }

    /// Straight-alpha `src` over `dst`.
    pub fn blit_over(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
    ) {
        let bind = self.bind(device, &PassUniforms::default().bytes(), src);
        Self::run(encoder, &self.blit, &bind, dst, false);
    }

    /// Straight-alpha `src` over `dst`, faded to `opacity`.
    pub fn blit_over_faded(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        opacity: f32,
    ) {
        let u = PassUniforms {
            premul: 1,
            gain: opacity,
            ..Default::default()
        };
        let bind = self.bind(device, &u.bytes(), src);
        Self::run(encoder, &self.blit, &bind, dst, false);
    }

    /// `src` box-downsampled by `step` into `dst` (premultiplying first when
    /// `premul`).
    pub fn down(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        step: u32,
        premul: bool,
    ) {
        let u = PassUniforms {
            step,
            premul: u32::from(premul),
            ..Default::default()
        };
        let bind = self.bind(device, &u.bytes(), src);
        Self::run(encoder, &self.down, &bind, dst, true);
    }

    /// One Gaussian axis (`dir` = (1,0) or (0,1)), `sigma` in texels.
    pub fn blur(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        dir: [f32; 2],
        sigma: f32,
    ) {
        let sigma = sigma.max(0.5);
        let taps = (sigma * 3.0).ceil().clamp(1.0, 32.0) as i32;
        let u = PassUniforms {
            dir,
            sigma,
            taps,
            ..Default::default()
        };
        let bind = self.bind(device, &u.bytes(), src);
        Self::run(encoder, &self.blur, &bind, dst, true);
    }

    /// Blurred light `src` added over `dst`, times `gain` — times `inside`
    /// where `mask` (the emitters) covers.
    #[allow(clippy::too_many_arguments)]
    pub fn bloom(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        mask: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        gain: f32,
        inside: f32,
    ) {
        let u = PassUniforms {
            gain,
            inside,
            ..Default::default()
        };
        let bind = self.bind2(device, &u.bytes(), src, mask, &self.empty_data);
        Self::run(encoder, &self.bloom, &bind, dst, false);
    }

    /// Run the effect `pipeline` over the subtree in `src`, over `dst`,
    /// with its input values `data`, inside `scissor` (the node's box).
    #[allow(clippy::too_many_arguments)]
    pub fn effect_pass(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        uniforms: &FxUniforms,
        data: &[f32],
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        scissor: Option<[u32; 4]>,
    ) {
        if scissor.is_some_and(|s| s[2] == 0 || s[3] == 0) {
            return;
        }
        let buffer = (!data.is_empty()).then(|| {
            let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("frame-fx-data"),
                contents: &bytes,
                usage: wgpu::BufferUsages::STORAGE,
            })
        });
        let bind = self.bind2(
            device,
            &uniforms.bytes(),
            src,
            src,
            buffer.as_ref().unwrap_or(&self.empty_data),
        );
        Self::run_in(encoder, pipeline, &bind, dst, false, scissor);
    }
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    vs: &str,
    fs: &str,
    format: wgpu::TextureFormat,
    blend: Blend,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("frame-fx"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend.state(),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Compile an effect module; validation errors come back as `Err` instead of
/// reaching the device's uncaptured-error handler.
fn compile(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: &str,
) -> Result<wgpu::RenderPipeline, String> {
    // Parse + validate with naga first for a readable message (and so an
    // invalid module never reaches the backend).
    let module =
        wgpu::naga::front::wgsl::parse_str(source).map_err(|e| e.emit_to_string(source))?;
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .map_err(|e| e.emit_to_string(source))?;
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("frame-fx-effect"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let p = pipeline(
        device,
        layout,
        &module,
        "fx_vs_main",
        "fx_fs_main",
        TARGET,
        Blend::Over,
    );
    match pollster::block_on(scope.pop()) {
        Some(e) => Err(e.to_string()),
        None => Ok(p),
    }
}
