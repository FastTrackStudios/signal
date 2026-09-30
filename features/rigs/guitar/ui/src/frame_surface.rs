//! A frame surface in the rig — a page designed in frame, running live.
//!
//! [`FrameSurface`] mounts one page of a frame design (`<name>.fm` beside its
//! `*.catalog.json`) as a Blitz custom widget: `frame_live::LiveSurface`
//! renders it through frame's own effects stage on the window's wgpu device,
//! into a texture Blitz composites, and runs its controls from the pointer.
//!
//! The files are watched: regenerate or edit the design and the panel swaps
//! the new one in within a quarter second, values kept — so a surface is
//! designed against the running app instead of by relaunching it.
//!
//! Where designs come from: `SIGNAL_FRAME_DIR`, else the blues rig in the
//! frame checkout beside this one (`../../frame/examples/plugins/rig-blues`,
//! as the processor and daw checkouts are found). Machine-specific while
//! the rig pages are being designed; they ship as data later.
//!
//! The controls are not yet bound to rig blocks: an edit moves the control
//! and is logged, and nothing reaches the engine.

use dioxus::prelude::*;

/// The rig-blues pages in the frame checkout, unless `SIGNAL_FRAME_DIR` says
/// otherwise.
#[cfg(not(target_arch = "wasm32"))]
fn design_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("SIGNAL_FRAME_DIR") {
        return dir.into();
    }
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // features/rigs/guitar/ui → the repo root → its parent → frame.
    here.join("../../../../../../frame/examples/plugins/rig-blues")
}

/// One page of a frame design, `name` (its `.fm` file's stem), filling
/// its box — fitted whole and centred. `on_edit` hears every param edit
/// the page makes, as (address, normalized value).
#[component]
pub fn FrameSurface(name: String, on_edit: Option<EventHandler<(String, f64)>>) -> Element {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::mount(name, on_edit)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (name, on_edit);
        rsx! { div { style: "width: 100%; height: 100%;" } }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::cell::RefCell;
    use std::rc::Rc;

    use anyrender::{PaintScene, RenderContext, ResourceId, Scene};
    use blitz_dom::Widget;
    use blitz_dom::node::ComputedStyles;
    use blitz_traits::events::{BlitzWheelDelta, UiEvent};
    use dioxus::prelude::*;
    use frame_live::{Call, LiveSurface};
    use kurbo::{Affine, Rect};
    use vello::peniko::{Fill, ImageBrush, ImageSampler};

    /// Pixels per wheel notch when the wheel reports pixels.
    const PX_PER_NOTCH: f64 = 40.0;

    type Shared = Rc<RefCell<Result<LiveSurface, String>>>;
    /// Edits the widget heard, for the component to hand on.
    type Edits = Rc<RefCell<Vec<(String, f64)>>>;

    pub(super) fn mount(name: String, on_edit: Option<EventHandler<(String, f64)>>) -> Element {
        crate::fx_viz::use_repaint_clock();
        let live: Shared = use_hook(|| {
            let (markup, catalog) = frame_live::face_paths(&super::design_dir(), &name);
            let opened = LiveSurface::open(&markup, catalog).map_err(|e| {
                tracing::warn!(target: "frame", surface = %markup.display(), error = %e, "frame surface did not open");
                e
            });
            Rc::new(RefCell::new(opened))
        });
        let edits: Edits = use_hook(|| Rc::new(RefCell::new(Vec::new())));
        let attr = use_hook(|| dioxus_native_dom::CustomWidgetAttr::new(FrameWidget { live: Rc::clone(&live), edits: Rc::clone(&edits), target: None }));
        // The repaint clock runs this body ~40 times a second; the edits
        // go out from a task, not from inside a render.
        let heard: Vec<(String, f64)> = std::mem::take(&mut *edits.borrow_mut());
        if let Some(handler) = on_edit
            && !heard.is_empty()
        {
            spawn(async move {
                for edit in heard {
                    handler.call(edit);
                }
            });
        }
        let error = live.borrow().as_ref().err().cloned();
        rsx! {
            if let Some(e) = error {
                div { style: "width: 100%; height: 100%; color: #e57373; font-size: 11px; padding: 8px;", "frame: {e}" }
            } else {
                object { style: "display: block; width: 100%; height: 100%;", data: attr }
            }
        }
    }

    struct FrameWidget {
        live: Shared,
        edits: Edits,
        /// The texture Blitz holds for us, and its size.
        target: Option<(ResourceId, (u32, u32))>,
    }

    /// The window's device and queue, from whatever the renderer boxed.
    fn device_and_queue(ctx: Box<dyn std::any::Any>) -> Option<(wgpu::Device, wgpu::Queue)> {
        let ctx = match ctx.downcast::<wgpu_context::DeviceHandle>() {
            Ok(h) => return Some((h.device.clone(), h.queue.clone())),
            Err(ctx) => ctx,
        };
        ctx.downcast::<vello::util::DeviceHandle>().ok().map(|h| (h.device.clone(), h.queue.clone()))
    }

    /// Shift = fine, Ctrl/Cmd = alternate (reset-on-click), as frame's player.
    fn mods(m: blitz_traits::events::Modifiers) -> frame_live::Modifiers {
        frame_live::Modifiers { fine: m.shift(), alt: m.ctrl() || m.meta(), option: m.alt() }
    }

    fn hand_on(edits: &Edits, calls: Vec<Call>) {
        for call in calls {
            if let Call::SetParam { addr, value, .. } = &call {
                tracing::debug!(target: "frame", %addr, value, "frame edit");
                edits.borrow_mut().push((addr.to_string(), *value));
            }
        }
    }

    impl Widget for FrameWidget {
        fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
            let Some((device, queue)) = render_ctx.renderer_specific_context().and_then(device_and_queue) else {
                tracing::warn!(target: "frame", "renderer has no wgpu device: frame surface stays blank");
                return;
            };
            if let Ok(live) = self.live.borrow_mut().as_mut()
                && let Err(e) = live.attach_gpu(device, queue)
            {
                tracing::warn!(target: "frame", error = %e, "frame renderer did not start");
            }
            self.target = None;
        }

        fn destroy_surfaces(&mut self) {
            if let Ok(live) = self.live.borrow_mut().as_mut() {
                live.detach_gpu();
            }
            self.target = None;
        }

        fn handle_event(&mut self, event: &UiEvent) {
            let mut guard = self.live.borrow_mut();
            let Ok(live) = guard.as_mut() else { return };
            match event {
                UiEvent::PointerDown(e) => live.pointer_down(f64::from(e.element.x), f64::from(e.element.y), mods(e.mods)),
                UiEvent::PointerMove(e) => live.pointer_move(f64::from(e.element.x), f64::from(e.element.y), mods(e.mods)),
                UiEvent::PointerUp(_) => live.pointer_up(),
                UiEvent::PointerCancel(_) => live.cancel(),
                UiEvent::Wheel(e) => {
                    let notches = match e.delta {
                        BlitzWheelDelta::Lines(_, y) => -y,
                        BlitzWheelDelta::Pixels(_, y) => -y / PX_PER_NOTCH,
                    };
                    live.wheel(f64::from(e.element.x), f64::from(e.element.y), notches, mods(e.mods));
                }
                _ => {}
            }
            hand_on(&self.edits, live.take_calls());
        }

        fn paint(&mut self, render_ctx: &mut dyn RenderContext, _styles: &ComputedStyles, width: u32, height: u32, scale: f64) -> Scene {
            let mut scene = Scene::new();
            if width < 2 || height < 2 {
                return scene;
            }
            let mut guard = self.live.borrow_mut();
            let Ok(live) = guard.as_mut() else { return scene };
            if live.poll_reload() {
                tracing::info!(target: "frame", "frame surface reloaded");
            }
            if let Some(e) = live.error() {
                tracing::warn!(target: "frame", error = %e, "frame reload failed; keeping the last good design");
            }
            let stale = self.target.is_none_or(|(_, size)| size != (width, height));
            if stale {
                tracing::debug!(target: "frame", width, height, scale, "frame surface size");
            }
            if live.has_gpu() && (stale || live.needs_redraw()) {
                match live.render(width, height, scale) {
                    Ok((texture, fresh)) => {
                        if fresh || self.target.is_none() {
                            if let Some((old, _)) = self.target.take() {
                                render_ctx.unregister_resource(old);
                            }
                            match render_ctx.try_register_custom_resource(Box::new(texture)) {
                                Ok(id) => self.target = Some((id, (width, height))),
                                Err(e) => tracing::warn!(target: "frame", error = ?e, "Blitz would not take the frame texture"),
                            }
                        }
                    }
                    Err(e) => tracing::warn!(target: "frame", error = %e, "frame render failed"),
                }
                hand_on(&self.edits, live.take_calls());
            }
            if let Some((id, _)) = self.target {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    anyrender::Paint::Resource(ImageBrush { image: id, sampler: ImageSampler::default() }),
                    None,
                    &Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
                );
            }
            scene
        }
    }
}
