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
//! Where designs come from: `SIGNAL_FRAME_DIR`, else frame's generated
//! faces in the frame checkout beside this one
//! (`../../frame/examples/plugins`, as the processor and daw checkouts are
//! found); a surface is named by its path under that, `rig-faces/<file>`.
//! Machine-specific while the faces are being designed; they ship as data
//! later.
//!
//! Values cross in the params' own units: `values` in (a block's `drive`,
//! its level in dB…), `on_edit` out — the face's catalog ranges convert.

use dioxus::prelude::*;

/// Whether the rig is playing — the faces' lamps, tape and wavefronts are
/// the unit at work, so with it stopped they hold still and the window
/// stops redrawing them (a phone sat idle at ~35 animated widgets, 30 times
/// a second). Absent, faces animate as before.
#[derive(Clone, Copy)]
pub struct FacesMove(pub Signal<bool>);

/// frame's generated faces in the frame checkout, unless
/// `SIGNAL_FRAME_DIR` says otherwise.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn design_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("SIGNAL_FRAME_DIR") {
        return dir.into();
    }
    // The frame checkout beside the repo: the nearest ancestor of this crate
    // with `frame/examples/plugins` in it (features/rigs/guitar/ui → the repo
    // root → its parent). A fixed count of `..` went one level too far and
    // every face on the desktop drew as its plain fallback.
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("frame/examples/plugins"))
        .find(|d| d.is_dir())
        .unwrap_or_else(|| here.join("../../../../../frame/examples/plugins"))
}

/// One frame face, `name` (its path under the faces directory without
/// `.fm`, e.g. `rig-faces/22-amp-vibroverb`), filling its box — fitted
/// whole and centred. `values` are the params' values in their own units,
/// by the face's addresses; `on_edit` hears every edit the face makes,
/// the same way.
#[component]
pub fn FrameSurface(
    name: String,
    #[props(default)] values: Vec<(String, f64)>,
    on_edit: Option<EventHandler<(String, f64)>>,
    /// Fill the box exactly (a picture) instead of fitting the face whole.
    #[props(default)]
    stretch: bool,
    /// The same face laid out for other proportions — (name, width ÷
    /// height) — for a box that isn't the face's shape: the widget draws
    /// whichever fits its box best, so the face reflows as the box resizes
    /// rather than shrinking into it.
    #[props(default)]
    variants: Vec<(String, f64)>,
    /// Lay the face out for the box's proportions (frame's responsive
    /// mode): its picture widens or narrows, its controls keep their shape.
    #[props(default)]
    responsive: bool,
    /// Text the face shows (a nameplate's preset): (address, text).
    #[props(default)]
    texts: Vec<(String, String)>,
) -> Element {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Keyed by the face: another face (a lane switched to another
        // algorithm) is a new surface, opened on it, not the old one kept.
        // A one-item list, so the key is honoured wherever this sits.
        let key = format!("{name}|{}", variants.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(","));
        rsx! {
            for k in [key] {
                NativeFace { key: "{k}", name: name.clone(), values: values.clone(), on_edit, stretch, variants: variants.clone(), responsive, texts: texts.clone() }
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (name, values, on_edit, stretch, variants, responsive, texts);
        rsx! { div { style: "width: 100%; height: 100%;" } }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[component]
fn NativeFace(name: String, values: Vec<(String, f64)>, on_edit: Option<EventHandler<(String, f64)>>, stretch: bool, variants: Vec<(String, f64)>, responsive: bool, texts: Vec<(String, String)>) -> Element {
    native::mount(name, values, on_edit, stretch, variants, responsive, texts)
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
    use frame_live::LiveSurface;
    use kurbo::{Affine, Rect};
    use vello::peniko::{Fill, ImageBrush, ImageSampler};

    /// Pixels per wheel notch when the wheel reports pixels.
    const PX_PER_NOTCH: f64 = 40.0;

    type Shared = Rc<RefCell<Result<Surfaces, String>>>;

    /// A face's surfaces: the one design, or its variants by proportion
    /// (width ÷ height), and which is drawn.
    struct Surfaces {
        all: Vec<(f64, LiveSurface)>,
        active: usize,
    }

    impl Surfaces {
        fn active(&self) -> &LiveSurface {
            &self.all[self.active].1
        }

        fn active_mut(&mut self) -> &mut LiveSurface {
            &mut self.all[self.active].1
        }

        fn each_mut(&mut self) -> impl Iterator<Item = &mut LiveSurface> {
            self.all.iter_mut().map(|(_, l)| l)
        }

        /// Draw the variant closest to a `w` × `h` box; whether that changed.
        fn pick(&mut self, w: u32, h: u32) -> bool {
            if self.all.len() < 2 || h == 0 {
                return false;
            }
            let want = f64::from(w) / f64::from(h);
            let best = self
                .all
                .iter()
                .enumerate()
                .min_by(|(_, (a, _)), (_, (b, _))| (a.ln() - want.ln()).abs().total_cmp(&(b.ln() - want.ln()).abs()))
                .map_or(0, |(i, _)| i);
            let changed = best != self.active;
            self.active = best;
            changed
        }
    }

    fn open_face(name: &str, stretch: bool, responsive: bool) -> Result<LiveSurface, String> {
        let path = super::design_dir().join(name);
        let dir = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
        let stem = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (markup, catalog) = frame_live::face_paths(&dir, &stem);
        LiveSurface::open(&markup, catalog)
            .map(|mut l| {
                l.set_stretch(stretch);
                l.set_responsive(responsive);
                l.set_max_fps(max_fps());
                l
            })
            .map_err(|e| {
                tracing::warn!(target: "frame", surface = %markup.display(), error = %e, "frame surface did not open");
                e
            })
    }
    /// Edits the widget heard, for the component to hand on.
    type Edits = Rc<RefCell<Vec<(String, f64)>>>;

    /// The UI's frame rate: `FTS_MAX_FPS`, as the window paces its own
    /// redraws (30 unless set; 0 = unpaced).
    fn max_fps() -> u32 {
        std::env::var("FTS_MAX_FPS").ok().and_then(|v| v.parse().ok()).unwrap_or(30)
    }

    /// How often the design's files are checked for a new version.
    const RELOAD_POLL: std::time::Duration = std::time::Duration::from_millis(250);

    pub(super) fn mount(name: String, values: Vec<(String, f64)>, on_edit: Option<EventHandler<(String, f64)>>, stretch: bool, variants: Vec<(String, f64)>, responsive: bool, texts: Vec<(String, String)>) -> Element {
        let live: Shared = use_hook(|| {
            let opened = if variants.is_empty() {
                open_face(&name, stretch, responsive).map(|l| Surfaces { all: vec![(1.0, l)], active: 0 })
            } else {
                variants
                    .iter()
                    .map(|(n, aspect)| open_face(n, stretch, responsive).map(|l| (*aspect, l)))
                    .collect::<Result<Vec<_>, _>>()
                    .map(|all| Surfaces { all, active: 0 })
            };
            Rc::new(RefCell::new(opened))
        });
        let edits: Edits = use_hook(|| Rc::new(RefCell::new(Vec::new())));
        // The face redraws itself: its widget tells Blitz when it has
        // something new to draw or is moving, and hands edits back through
        // `update`. All this loop does is notice a regenerated design; the
        // reloaded face is then new to draw.
        let update = use_hook(dioxus_core::schedule_update);
        use_future({
            let live: Shared = Rc::clone(&live);
            move || {
                let live: Shared = Rc::clone(&live);
                async move {
                    loop {
                        architect::platform::sleep(RELOAD_POLL).await;
                        if let Ok(s) = live.borrow_mut().as_mut() {
                            for l in s.each_mut() {
                                if l.poll_reload() {
                                    tracing::info!(target: "frame", "frame surface reloaded");
                                }
                            }
                        }
                    }
                }
            }
        });
        // The block's values, whenever they differ from what was last
        // given (a param being dragged keeps the hand's).
        let mut applied: Signal<Vec<(String, f64)>> = use_signal(Vec::new);
        if !crate::frame_surface::same_values(&applied.peek(), &values) {
            if let Ok(s) = live.borrow_mut().as_mut() {
                for l in s.each_mut() {
                    l.apply_real(values.clone());
                }
            }
            applied.set(values);
        }
        // Its text, likewise.
        let mut applied_texts: Signal<Vec<(String, String)>> = use_signal(Vec::new);
        if *applied_texts.peek() != texts {
            if let Ok(s) = live.borrow_mut().as_mut() {
                for l in s.each_mut() {
                    l.apply_texts(texts.clone());
                }
            }
            applied_texts.set(texts);
        }
        // A face always moves: its lamps, tape and wavefronts are the unit
        // at work, not a picture of the audio, and each face holds its own
        // still when its effect is off (`param(<ns>/on)`). Only a face-less
        // visualiser waits for audio (`fts_audio_ui::animate`). A face is a
        // layer of its own, so moving costs one texture, not the page.
        // Still while the rig is stopped (`FacesMove`).
        let moving = try_use_context::<super::FacesMove>().is_none_or(|m| (m.0)());
        if let Ok(s) = live.borrow_mut().as_mut() {
            for l in s.each_mut() {
                l.set_animate(moving);
            }
        }
        let attr = use_hook(|| dioxus_native_dom::CustomWidgetAttr::new(FrameWidget { live: Rc::clone(&live), edits: Rc::clone(&edits), update: update.clone(), target: None }));
        // The edits go out from a task, not from inside a render.
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
        /// Re-renders the component, which hands the edits on.
        update: std::sync::Arc<dyn Fn() + Send + Sync>,
        /// The texture Blitz holds for us, its size, and which variant's.
        target: Option<(ResourceId, (u32, u32), usize)>,
    }

    /// The window's device and queue, from whatever the renderer boxed.
    /// The host's device, when Vello can render on it: its compute stages
    /// need indirect execution, which the iOS simulator's Metal lacks (wgpu
    /// aborts the app at the first face). Without it the face paints its
    /// vectors into the page instead (`paint_vectors`).
    fn device_and_queue(ctx: Box<dyn std::any::Any>) -> Option<(wgpu::Device, wgpu::Queue)> {
        let vello_can = |adapter: &wgpu::Adapter| {
            adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::INDIRECT_EXECUTION)
        };
        let ctx = match ctx.downcast::<wgpu_context::DeviceHandle>() {
            Ok(h) => return vello_can(&h.adapter).then(|| (h.device.clone(), h.queue.clone())),
            Err(ctx) => ctx,
        };
        ctx.downcast::<vello::util::DeviceHandle>()
            .ok()
            .filter(|h| vello_can(h.adapter()))
            .map(|h| (h.device.clone(), h.queue.clone()))
    }

    /// Shift = fine, Ctrl/Cmd = alternate (reset-on-click), as frame's player.
    fn mods(m: blitz_traits::events::Modifiers) -> frame_live::Modifiers {
        frame_live::Modifiers { fine: m.shift(), alt: m.ctrl() || m.meta(), option: m.alt() }
    }

    fn hand_on(edits: &Edits, live: &mut LiveSurface, update: &(dyn Fn() + Send + Sync)) {
        let mut any = false;
        for (addr, value) in live.take_edits() {
            tracing::debug!(target: "frame", %addr, value, "frame edit");
            edits.borrow_mut().push((addr, value));
            any = true;
        }
        if any {
            update();
        }
    }

    impl Widget for FrameWidget {
        fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
            // No device Vello can render on: the face paints its vectors
            // into the page (see `paint`).
            let Some((device, queue)) = render_ctx.renderer_specific_context().and_then(device_and_queue) else {
                tracing::info!(target: "frame", "no device for Vello: frame surface paints vectors");
                return;
            };
            if let Ok(s) = self.live.borrow_mut().as_mut() {
                for live in s.each_mut() {
                    if let Err(e) = live.attach_gpu(device.clone(), queue.clone()) {
                        tracing::warn!(target: "frame", error = %e, "frame renderer did not start");
                    }
                }
            }
            self.target = None;
        }

        fn destroy_surfaces(&mut self) {
            if let Ok(s) = self.live.borrow_mut().as_mut() {
                for live in s.each_mut() {
                    live.detach_gpu();
                }
            }
            self.target = None;
        }

        fn handle_event(&mut self, event: &UiEvent) {
            let mut guard = self.live.borrow_mut();
            let Ok(surfaces) = guard.as_mut() else { return };
            let live = surfaces.active_mut();
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
            hand_on(&self.edits, live, &*self.update);
        }

        /// The texture the face is: Blitz can draw it over the page as a
        /// layer, so a frame where only faces move repaints only them.
        fn composite_texture(&self) -> Option<ResourceId> {
            self.target.map(|(id, _, _)| id)
        }

        /// Something new to draw or still moving: Blitz draws a frame for
        /// it, and keeps drawing (paced) while it moves.
        fn needs_redraw(&self) -> bool {
            self.live.try_borrow().is_ok_and(|s| s.as_ref().is_ok_and(|s| s.active().is_moving()))
        }

        fn paint(&mut self, render_ctx: &mut dyn RenderContext, _styles: &ComputedStyles, width: u32, height: u32, scale: f64) -> Scene {
            let mut scene = Scene::new();
            if width < 2 || height < 2 {
                return scene;
            }
            let mut guard = self.live.borrow_mut();
            let Ok(surfaces) = guard.as_mut() else { return scene };
            // The variant that fits this box (a face laid out for it).
            surfaces.pick(width, height);
            let active = surfaces.active;
            let live = surfaces.active_mut();
            let before = live.error().map(str::to_string);
            if live.poll_reload() {
                tracing::info!(target: "frame", "frame surface reloaded");
            }
            if let Some(e) = live.error()
                && before.as_deref() != Some(e)
            {
                tracing::warn!(target: "frame", error = %e, "frame reload failed; keeping the last good design");
            }
            let stale = self.target.is_none_or(|(_, size, index)| size != (width, height) || index != active);
            if stale {
                tracing::debug!(target: "frame", width, height, scale, "frame surface size");
            }
            if live.has_gpu() && (stale || live.needs_redraw()) {
                match live.render(width, height, scale) {
                    Ok((texture, fresh)) => {
                        if fresh || stale {
                            if let Some((old, _, _)) = self.target.take() {
                                render_ctx.unregister_resource(old);
                            }
                            match render_ctx.try_register_custom_resource(Box::new(texture)) {
                                Ok(id) => self.target = Some((id, (width, height), active)),
                                Err(e) => tracing::warn!(target: "frame", error = ?e, "Blitz would not take the frame texture"),
                            }
                        }
                    }
                    Err(e) => tracing::warn!(target: "frame", error = %e, "frame render failed"),
                }
                hand_on(&self.edits, live, &*self.update);
            }
            // No Vello GPU to render with (the iOS simulator: no indirect
            // execution): the face's vectors go straight into this scene, for
            // whatever renderer draws it — vello-hybrid there.
            if !live.has_gpu() {
                live.paint_vectors(&mut scene, width, height, scale);
                hand_on(&self.edits, live, &*self.update);
                return scene;
            }
            if let Some((id, _, _)) = self.target {
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

/// Whether two value lists are the same — NaN counting as itself. `!=` on
/// floats calls a NaN different from every NaN, so a face carrying one (an
/// amp's unset level) re-applied its values every render, and the re-apply
/// asked for the next render: the Amp page redrew at the frame rate for as
/// long as it was up, and the phone's main thread had no turn for taps.
fn same_values(a: &[(String, f64)], b: &[(String, f64)]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|((na, va), (nb, vb))| {
            na == nb && (va == vb || (va.is_nan() && vb.is_nan()))
        })
}

