//! A painted panel in a browser: the widget itself, on a canvas.
//!
//! Natively a painted panel is a Blitz *custom widget*: the host calls
//! `can_create_surfaces` once (where the widget takes a GPU device and builds
//! its [`ShaderSurface`](crate::shader::ShaderSurface)), then `paint` each
//! frame, and composites the returned [`anyrender::Scene`] into its own pass.
//!
//! A DOM renderer cannot be handed a scene at all, so the browser gets a
//! surface of its own: a `<canvas>` with vello on WebGPU behind it, driven
//! through the same two calls. The panel is the widget — the same struct the
//! plugin mounts, running the same code — so the browser gets the shader
//! layers too, not a vector imitation of them.
//!
//! **One GPU for every canvas.** A page mounts many panels — the Control
//! view alone has fourteen. Each used to own an `anyrender_vello`
//! `VelloWindowRenderer`, and with it a wgpu device *and* a `vello::Renderer`,
//! whose construction compiles every one of vello's compute pipelines:
//! fourteen devices, fourteen compiles, fourteen copies of the same shaders.
//! A WebGPU device can configure any number of canvases, so there is one
//! device and one vello renderer per page ([`Gpu`]), and a canvas owns only
//! its surface.
//!
//! `anyrender_vello` has no way to share a device between window renderers,
//! and its painter only resolves a panel's registered textures from a
//! constructor private to the crate. Neither is needed: a panel's scene is a
//! plain [`anyrender::Scene`] recording, so the texture ids in it
//! (`Paint::Resource`) are swapped here for the images the shared renderer
//! registered, and the result replays through the public
//! `VelloScenePainter::new`. The widgets see exactly what they saw before —
//! a [`RenderContext`] whose `renderer_specific_context()` is a real
//! `wgpu_context::DeviceHandle`, and whose `try_register_custom_resource`
//! takes the texture their shader drew into.
//!
//! The clock matches the native one (the effects' `use_repaint_clock`): a
//! visualiser animates without anything in the DOM changing, so the canvas
//! repaints on a timer rather than on a re-render.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use anyrender::recording::RenderCommand;
use anyrender::{Paint, PaintScene, RenderContext, ResourceId, Scene};
use anyrender_vello::VelloScenePainter;
use dioxus::prelude::*;
use kurbo::Affine;
use peniko::{ImageBrush, ImageData};
use wasm_bindgen::JsCast;
use wgpu_context_07::{
    DeviceHandle, SurfaceRenderer, SurfaceRendererConfiguration, TextureConfiguration, WGPUContext,
};

/// Repaints a second — the native clock's rate.
const FPS: u32 = 40;

thread_local! {
    /// Canvas ids are unique per document, and a panel needs one to find its
    /// own canvas again from the timer.
    static NEXT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };

    /// The wgpu instance, and the features the device is asked for — the
    /// ones `anyrender_vello` asks for, so the device is the one a panel got
    /// from a window renderer before.
    static CONTEXT: WGPUContext = WGPUContext::with_features_and_limits(
        Some(wgpu::Features::CLEAR_TEXTURE | wgpu::Features::PIPELINE_CACHE),
        None,
    );

    /// The page's one GPU (see the module docs).
    static GPU: RefCell<GpuState> = const { RefCell::new(GpuState::Idle) };
}

/// What a painted panel does, on any host: the two calls Blitz makes.
///
/// Implemented by the effects' widgets themselves (`CompWidget`,
/// `DelayWidget`, …), so the browser runs the plugin's own painting code.
pub trait CanvasPanel: 'static {
    /// Take a GPU device, if the host has one — where a widget builds its
    /// shader surface. Called once per canvas, when the GPU is up.
    fn can_create_surfaces(&mut self, _ctx: &mut dyn RenderContext) {}

    /// Record this frame. `width`/`height` are the canvas's backing store
    /// (device pixels), `scale` the device pixel ratio.
    fn paint(&mut self, ctx: &mut dyn RenderContext, width: u32, height: u32, scale: f64) -> Scene;
}

/// A [`CanvasPanel`] as a component prop: shared, and compared by identity.
#[derive(Clone)]
pub struct Panel(Rc<RefCell<dyn CanvasPanel>>);

impl Panel {
    #[must_use]
    pub fn new(panel: impl CanvasPanel) -> Self {
        Self(Rc::new(RefCell::new(panel)))
    }
}

impl PartialEq for Panel {
    /// Same panel, not "same contents": a widget is a thing with state, and
    /// two of them are never interchangeable.
    fn eq(&self, other: &Self) -> bool {
        std::ptr::addr_eq(Rc::as_ptr(&self.0), Rc::as_ptr(&other.0))
    }
}

// ── The shared GPU ──────────────────────────────────────────────────────────

/// One device, one vello renderer, and every texture a panel has handed over.
struct Gpu {
    handle: DeviceHandle,
    renderer: vello::Renderer,
    /// A panel's shader output, by the id `try_register_custom_resource`
    /// answered with. Shared, because the renderer that holds the images is.
    textures: HashMap<ResourceId, ImageData>,
}

/// Where the shared GPU is in its life. Asking for a device is async on the
/// web, so the first canvas to draw starts it and every canvas waits on it.
enum GpuState {
    Idle,
    Pending(Rc<RefCell<Option<Result<Gpu, String>>>>),
    Ready(Rc<RefCell<Gpu>>),
    /// No WebGPU here, or the adapter refused. Logged once; panels stay
    /// blank rather than asking again every frame.
    Failed,
}

/// The shared GPU if it is up; starts it the first time it is asked.
fn shared_gpu() -> Option<Rc<RefCell<Gpu>>> {
    GPU.with(|state| {
        let mut state = state.borrow_mut();
        match &*state {
            GpuState::Ready(gpu) => return Some(gpu.clone()),
            GpuState::Failed => return None,
            GpuState::Pending(slot) => {
                let done = slot.borrow_mut().take()?;
                return match done {
                    Ok(gpu) => {
                        let gpu = Rc::new(RefCell::new(gpu));
                        *state = GpuState::Ready(gpu.clone());
                        Some(gpu)
                    }
                    Err(error) => {
                        tracing::error!("scene canvas: no GPU for painted panels: {error}");
                        *state = GpuState::Failed;
                        None
                    }
                };
            }
            GpuState::Idle => {}
        }

        // No compatible surface: on WebGPU every adapter can present to every
        // canvas (a canvas context is configured *with* a device, not made
        // by one), and a future that borrowed one could not be spawned.
        let device = CONTEXT.with(|ctx| {
            DeviceHandle::new_from_compatible_surface(
                ctx.instance.clone(),
                None,
                ctx.extra_features(),
                ctx.override_limits(),
            )
        });
        let slot = Rc::new(RefCell::new(None));
        // Not dioxus's `spawn`: that task belongs to whichever component
        // asked first, and would die with it — taking every other canvas's
        // GPU along.
        wasm_bindgen_futures::spawn_local({
            let slot = slot.clone();
            async move {
                let gpu = match device.await {
                    Ok(handle) => vello::Renderer::new(
                        &handle.device,
                        vello::RendererOptions {
                            antialiasing_support: vello::AaSupport::all(),
                            use_cpu: false,
                            num_init_threads: None,
                            pipeline_cache: None,
                        },
                    )
                    .map(|renderer| Gpu {
                        handle,
                        renderer,
                        textures: HashMap::new(),
                    })
                    .map_err(|e| format!("vello renderer: {e:?}")),
                    Err(e) => Err(format!("wgpu device: {e:?}")),
                };
                *slot.borrow_mut() = Some(gpu);
            }
        });
        *state = GpuState::Pending(slot);
        None
    })
}

/// The render context a panel sees: the shared GPU, plus a note of which
/// registrations are this canvas's, so they can be returned when it goes.
struct PanelContext<'a> {
    gpu: &'a mut Gpu,
    owned: &'a mut Vec<ResourceId>,
}

impl RenderContext for PanelContext<'_> {
    fn try_register_custom_resource(
        &mut self,
        resource: Box<dyn std::any::Any>,
    ) -> Result<ResourceId, anyrender::RegisterResourceError> {
        let Ok(texture) = resource.downcast::<wgpu::Texture>() else {
            return Err(anyrender::RegisterResourceErrorKind::UnsupportedResourceKind.into());
        };
        let id = ResourceId::new();
        let image = self.gpu.renderer.register_texture(*texture);
        self.gpu.textures.insert(id, image);
        self.owned.push(id);
        Ok(id)
    }

    fn unregister_resource(&mut self, id: ResourceId) {
        if let Some(image) = self.gpu.textures.remove(&id) {
            self.gpu.renderer.unregister_texture(image);
        }
        self.owned.retain(|owned| *owned != id);
    }

    fn renderer_specific_context(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(self.gpu.handle.clone()))
    }
}

/// Swap every texture id in `scene` for the image it names, so the public
/// painter (which cannot see the registry) draws it. Returns the images
/// used: vello copies an override image into its atlas only when told the
/// texture changed, and a shader redraws it every frame.
fn resolve_resources(
    scene: &mut Scene,
    textures: &HashMap<ResourceId, ImageData>,
) -> Vec<ImageData> {
    let mut used = Vec::new();
    let mut resolve = |paint: &mut Paint| {
        if let Paint::Resource(brush) = paint {
            // An id the registry does not know — a texture from a canvas
            // that has gone — draws nothing, as the stock painter does.
            *paint = match textures.get(&brush.image) {
                Some(image) => {
                    used.push(image.clone());
                    Paint::Image(ImageBrush {
                        image: image.clone(),
                        sampler: brush.sampler,
                    })
                }
                None => Paint::Solid(peniko::Color::TRANSPARENT),
            };
        }
    };
    for command in &mut scene.commands {
        match command {
            RenderCommand::Fill(cmd) => resolve(&mut cmd.brush),
            RenderCommand::Stroke(cmd) => resolve(&mut cmd.brush),
            RenderCommand::GlyphRun(cmd) => resolve(&mut cmd.brush),
            _ => {}
        }
    }
    used
}

// ── One canvas ──────────────────────────────────────────────────────────────

/// A canvas's share of the GPU: its surface, and the textures its panel
/// registered.
struct Surface {
    target: SurfaceRenderer<'static>,
    gpu: Rc<RefCell<Gpu>>,
    /// Kept between frames for its allocations.
    scene: vello::Scene,
    /// Registrations to hand back when this canvas goes. With a renderer per
    /// canvas they died with it; the shared one outlives every canvas.
    owned: Vec<ResourceId>,
}

impl Drop for Surface {
    fn drop(&mut self) {
        let Ok(mut gpu) = self.gpu.try_borrow_mut() else {
            return;
        };
        for id in self.owned.drain(..) {
            if let Some(image) = gpu.textures.remove(&id) {
                gpu.renderer.unregister_texture(image);
            }
        }
    }
}

/// A canvas before its surface exists: the wgpu surface is made at once (it
/// is what the device request needs), the renderer when the GPU is up.
enum Slot {
    Empty,
    Waiting(wgpu::Surface<'static>),
    Live(Box<Surface>),
    /// The canvas could not be configured; logged once, left blank.
    Dead,
}

/// A painted panel on a canvas: the widget draws `FPS` times a second, with
/// the page's GPU behind it.
#[component]
pub fn SceneCanvas(
    /// The widget doing the painting.
    panel: Panel,
    /// Extra classes for the canvas element.
    #[props(default)]
    class: String,
) -> Element {
    let id = use_hook(|| {
        let n = NEXT_ID.with(|c| {
            let n = c.get();
            c.set(n + 1);
            n
        });
        format!("fts-scene-canvas-{n}")
    });
    let slot: Rc<RefCell<Slot>> = use_hook(|| Rc::new(RefCell::new(Slot::Empty)));

    use_future({
        let (id, slot, panel) = (id.clone(), slot.clone(), panel.clone());
        move || {
            let (id, slot, panel) = (id.clone(), slot.clone(), panel.clone());
            async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(1000 / FPS).await;
                    draw(&id, &slot, &panel);
                }
            }
        }
    });

    rsx! {
        canvas {
            id: "{id}",
            class: "{class}",
            // The element sizes itself; the backing store follows it in
            // `draw`, at the device's pixel ratio.
            //
            // `pointer-events: none` because this is a picture, not a
            // control: a positioned canvas paints above the gesture layer
            // that sits with it (the EQ's svg, the compressor's), and
            // without this it would swallow every press meant for that.
            style: "display: block; width: 100%; height: 100%; pointer-events: none;",
        }
    }
}

/// One frame: bring the surface up if it is not, size it, then let the
/// widget paint into it.
fn draw(id: &str, slot: &Rc<RefCell<Slot>>, panel: &Panel) {
    let Some(canvas) = canvas_by_id(id) else {
        return;
    };
    let (css_w, css_h) = (
        f64::from(canvas.client_width()),
        f64::from(canvas.client_height()),
    );
    if css_w < 1.0 || css_h < 1.0 {
        return;
    }
    let dpr = web_sys::window()
        .map_or(1.0, |w| w.device_pixel_ratio())
        .max(1.0);
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "a pixel count"
    )]
    let (w, h) = ((css_w * dpr) as u32, (css_h * dpr) as u32);

    let mut slot = slot.borrow_mut();
    if matches!(*slot, Slot::Empty) {
        canvas.set_width(w);
        canvas.set_height(h);
        let surface = CONTEXT.with(|ctx| {
            ctx.instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
        });
        match surface {
            Ok(surface) => *slot = Slot::Waiting(surface),
            Err(e) => {
                tracing::error!("scene canvas: no surface on #{id}: {e}");
                *slot = Slot::Dead;
                return;
            }
        }
    }

    if matches!(*slot, Slot::Waiting(_)) {
        let Some(gpu) = shared_gpu() else {
            return;
        };
        if let Slot::Waiting(surface) = std::mem::replace(&mut *slot, Slot::Dead) {
            if let Some(live) = bring_up(&canvas, id, (w, h), surface, gpu, panel) {
                *slot = Slot::Live(Box::new(live));
            }
        }
    }

    let Slot::Live(surface) = &mut *slot else {
        return;
    };

    if (surface.target.config.width, surface.target.config.height) != (w, h) {
        canvas.set_width(w);
        canvas.set_height(h);
        surface.target.resize(w, h);
    }

    let gpu_rc = surface.gpu.clone();
    let mut gpu = gpu_rc.borrow_mut();
    let mut scene = {
        let mut ctx = PanelContext {
            gpu: &mut gpu,
            owned: &mut surface.owned,
        };
        panel.0.borrow_mut().paint(&mut ctx, w, h, dpr)
    };
    let used = resolve_resources(&mut scene, &gpu.textures);
    VelloScenePainter::new(&mut surface.scene).append_scene(scene, Affine::IDENTITY);
    for image in &used {
        gpu.renderer.mark_override_image_dirty(image);
    }

    let Ok(view) = surface.target.target_texture_view() else {
        surface.target.clear_surface_texture();
        surface.scene.reset();
        return;
    };
    let Gpu {
        handle, renderer, ..
    } = &mut *gpu;
    let rendered = renderer.render_to_texture(
        &handle.device,
        &handle.queue,
        &surface.scene,
        &view,
        &vello::RenderParams {
            // Transparent, not vello's usual white: these panels are layers
            // over the page's own ground, and several paint none of their own.
            base_color: peniko::Color::TRANSPARENT,
            width: w,
            height: h,
            antialiasing_method: vello::AaConfig::Msaa16,
        },
    );
    drop(view);
    surface.scene.reset();
    match rendered {
        Ok(()) => {
            let _ = surface.target.maybe_blit_and_present();
        }
        Err(e) => tracing::warn!("scene canvas: #{id} did not render: {e:?}"),
    }
}

/// Configure a canvas's surface on the shared device, and offer its panel
/// the device — the same call, in the same order, Blitz makes natively.
fn bring_up(
    canvas: &web_sys::HtmlCanvasElement,
    id: &str,
    (w, h): (u32, u32),
    surface: wgpu::Surface<'static>,
    gpu: Rc<RefCell<Gpu>>,
    panel: &Panel,
) -> Option<Surface> {
    let handle = gpu.borrow().handle.clone();
    canvas.set_width(w);
    canvas.set_height(h);
    let target = SurfaceRenderer::new(
        surface,
        SurfaceRendererConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            formats: vec![
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Bgra8Unorm,
            ],
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        },
        // vello writes a storage texture, which a canvas's own is not; the
        // surface renderer blits from this one.
        Some(TextureConfiguration {
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        }),
        handle,
    )
    .inspect_err(|e| tracing::error!("scene canvas: cannot configure #{id}: {e:?}"))
    .ok()?;
    let mut live = Surface {
        target,
        gpu,
        scene: vello::Scene::new(),
        owned: Vec::new(),
    };
    {
        let mut gpu = live.gpu.borrow_mut();
        let mut ctx = PanelContext {
            gpu: &mut gpu,
            owned: &mut live.owned,
        };
        panel.0.borrow_mut().can_create_surfaces(&mut ctx);
    }
    Some(live)
}

fn canvas_by_id(id: &str) -> Option<web_sys::HtmlCanvasElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(id)?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .ok()
}
