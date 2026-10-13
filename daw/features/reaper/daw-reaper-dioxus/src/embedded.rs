//! EmbeddedView — Dioxus rendering inside an existing REAPER window.
//!
//! Creates a child window parented to a REAPER HWND and renders a Dioxus
//! component tree into it. Unlike `DioxusOverlay`, this is not transparent
//! or click-through — it's a fully interactive view embedded inside a
//! REAPER panel, docker, or custom window.
//!
//! On Linux, creates an X11 child window via XReparentWindow.
//! On macOS, creates an NSView added as a subview.

use anyrender_vello::VelloScenePainter;
use blitz_dom::Document as _;
use blitz_paint::paint_scene;
use blitz_traits::shell::{ColorScheme, Viewport};
use crossbeam::channel::{Receiver, unbounded};
use daw_reaper_embed::{GpuError, GpuState};
use dioxus_native::prelude::*;
use dioxus_native::{DioxusDocument, DocumentConfig};
use futures_util::task::ArcWake;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use vello::Scene;

use crate::hot_reload::HotReloadState;
use crate::overlay::{DocumentMessage, DocumentProxy};

/// A Dioxus view embedded inside an existing REAPER HWND.
///
/// Supports hot reload via dioxus-devtools when the `hot-reload` feature
/// is enabled. Run `dx serve` to connect.
pub struct EmbeddedView {
    doc: DioxusDocument,
    /// Kept so the view can be [remounted](EmbeddedView::remount) — a
    /// panel that is closed and opened again should come back bound to
    /// whatever is selected *now*, not to whatever was selected the
    /// first time it opened.
    app: fn() -> Element,
    /// Same, and why these are `Fn` rather than `FnOnce`: a remount has
    /// to be able to inject them a second time.
    contexts: Vec<Rc<dyn Fn()>>,
    doc_message_rx: Receiver<DocumentMessage>,
    gpu: GpuState,
    scene: Scene,
    waker: std::task::Waker,
    needs_redraw: Arc<AtomicBool>,
    hot_reload: HotReloadState,
    animation_start: Instant,
    scale_factor: f32,
    width: u32,
    height: u32,
    /// Cached BGRA8 readback bytes (offscreen mode only). Reused across
    /// WM_PAINT blits so the window can repaint without re-rendering.
    readback: Vec<u8>,
    /// True on the frame after a successful offscreen render; the consumer
    /// uses this to call `InvalidateRect` so SWELL posts a fresh WM_PAINT.
    needs_blit: bool,
}

impl EmbeddedView {
    /// Create a new embedded view rendering into the given REAPER HWND.
    ///
    /// The `parent_hwnd` should be a valid SWELL HWND (e.g., from a dockable
    /// dialog or custom window).
    ///
    /// # Safety
    /// The `parent_hwnd` must be a valid window handle that remains valid for
    /// the lifetime of this `EmbeddedView`.
    pub fn new<W>(
        app: fn() -> Element,
        window: &W,
        width: u32,
        height: u32,
        contexts: Vec<Rc<dyn Fn()>>,
    ) -> Result<Self, GpuError>
    where
        W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle,
    {
        let width = width.max(1);
        let height = height.max(1);
        let scale_factor = daw_reaper_embed::display_scale_factor() as f32;

        // Create GPU state from the parent window handle
        let gpu = GpuState::new(window, width, height)?;

        // Set up document::Style message channel
        let (doc_tx, doc_rx) = unbounded();
        let doc_proxy = Rc::new(DocumentProxy::new(doc_tx));

        // Create the Dioxus document
        let vdom = VirtualDom::new(app);
        let viewport = Viewport::new(width, height, scale_factor, ColorScheme::Dark);

        let mut doc = DioxusDocument::new(
            vdom,
            DocumentConfig {
                viewport: Some(viewport),
                ..Default::default()
            },
        );

        // Inject contexts
        doc.vdom.in_scope(ScopeId::ROOT, || {
            provide_context(doc_proxy as Rc<dyn document::Document>);
            for ctx_fn in &contexts {
                ctx_fn();
            }
        });

        // Initial build
        doc.initial_build();

        // Process initial document::Style messages
        while let Ok(msg) = doc_rx.try_recv() {
            match msg {
                DocumentMessage::CreateHeadElement {
                    name,
                    attributes,
                    contents,
                } => {
                    doc.create_head_element(&name, &attributes, &contents);
                }
            }
        }

        doc.inner_mut().resolve(0.0);

        let needs_redraw = Arc::new(AtomicBool::new(true));
        let waker = futures_util::task::waker(Arc::new(EmbeddedWaker(needs_redraw.clone())));

        // Connect to dioxus-devtools for hot reload (if feature enabled)
        let mut hot_reload = HotReloadState::new();
        hot_reload.connect();

        Ok(Self {
            doc,
            app,
            contexts,
            doc_message_rx: doc_rx,
            gpu,
            scene: Scene::new(),
            waker,
            needs_redraw,
            hot_reload,
            animation_start: Instant::now(),
            scale_factor,
            width,
            height,
            readback: Vec::new(),
            needs_blit: false,
        })
    }

    /// Create an embedded view that renders offscreen (no surface).
    ///
    /// Used on Linux docked panels where SWELL owns the HWND and we must
    /// blit CPU bytes via `StretchBltFromMem` under `WM_PAINT`. Matches
    /// reaimgui's GDK offscreen-GL path (gdk_opengl.cpp:86-260).
    pub fn new_offscreen(
        app: fn() -> Element,
        width: u32,
        height: u32,
        contexts: Vec<Rc<dyn Fn()>>,
    ) -> Result<Self, GpuError> {
        let width = width.max(1);
        let height = height.max(1);
        let scale_factor = daw_reaper_embed::display_scale_factor() as f32;

        let gpu = GpuState::new_offscreen(width, height)?;

        let (doc_tx, doc_rx) = unbounded();
        let doc_proxy = Rc::new(DocumentProxy::new(doc_tx));

        let vdom = VirtualDom::new(app);
        let viewport = Viewport::new(width, height, scale_factor, ColorScheme::Dark);

        let mut doc = DioxusDocument::new(
            vdom,
            DocumentConfig {
                viewport: Some(viewport),
                ..Default::default()
            },
        );

        doc.vdom.in_scope(ScopeId::ROOT, || {
            provide_context(doc_proxy as Rc<dyn document::Document>);
            for ctx_fn in &contexts {
                ctx_fn();
            }
        });

        doc.initial_build();
        while let Ok(msg) = doc_rx.try_recv() {
            match msg {
                DocumentMessage::CreateHeadElement {
                    name,
                    attributes,
                    contents,
                } => {
                    doc.create_head_element(&name, &attributes, &contents);
                }
            }
        }
        doc.inner_mut().resolve(0.0);

        let needs_redraw = Arc::new(AtomicBool::new(true));
        let waker = futures_util::task::waker(Arc::new(EmbeddedWaker(needs_redraw.clone())));

        let mut hot_reload = HotReloadState::new();
        hot_reload.connect();

        Ok(Self {
            doc,
            app,
            contexts,
            doc_message_rx: doc_rx,
            gpu,
            scene: Scene::new(),
            waker,
            needs_redraw,
            hot_reload,
            animation_start: Instant::now(),
            scale_factor,
            width,
            height,
            readback: Vec::new(),
            needs_blit: false,
        })
    }

    /// Rebuild the component tree from scratch, keeping the window, the
    /// GPU surface and the size.
    ///
    /// A panel's view outlives its visibility — hiding only takes the
    /// HWND off screen, because throwing the GPU state away and building
    /// it again costs a fifth of a second on every dock toggle. The
    /// component tree, though, is not a resource: it is a *binding*. A
    /// tool panel resolves "the take I am editing" once, when it mounts,
    /// so a view that survives hiding goes on editing the take that was
    /// selected the first time it ever opened, however many items the
    /// user has clicked since. Reopening a tool is exactly when you mean
    /// "target what I have selected now" — so showing a hidden panel
    /// remounts it.
    ///
    /// Cheap: a fresh `VirtualDom` and one build, no GPU work.
    pub fn remount(&mut self) {
        let (doc_tx, doc_rx) = unbounded();
        let doc_proxy = Rc::new(DocumentProxy::new(doc_tx));

        let viewport = Viewport::new(
            self.width,
            self.height,
            self.scale_factor,
            ColorScheme::Dark,
        );
        let mut doc = DioxusDocument::new(
            VirtualDom::new(self.app),
            DocumentConfig {
                viewport: Some(viewport),
                ..Default::default()
            },
        );

        let contexts = self.contexts.clone();
        doc.vdom.in_scope(ScopeId::ROOT, || {
            provide_context(doc_proxy as Rc<dyn document::Document>);
            for ctx_fn in &contexts {
                ctx_fn();
            }
        });

        doc.initial_build();
        while let Ok(msg) = doc_rx.try_recv() {
            match msg {
                DocumentMessage::CreateHeadElement {
                    name,
                    attributes,
                    contents,
                } => {
                    doc.create_head_element(&name, &attributes, &contents);
                }
            }
        }
        doc.inner_mut().resolve(0.0);

        self.doc = doc;
        self.doc_message_rx = doc_rx;
        self.needs_redraw.store(true, Ordering::Relaxed);
    }

    /// Tick: poll vdom, resolve layout, render to GPU surface.
    ///
    /// Only does GPU work when the DOM has changed (waker fired or document::Style
    /// messages received). Skips painting when nothing is dirty.
    pub fn update(&mut self) {
        let animation_time = self.animation_start.elapsed().as_secs_f64();

        // Process hot reload messages (RSX patches from dx serve)
        let hot_changed = self.hot_reload.process_messages(&mut self.doc);

        // Poll the virtual DOM for async events
        let cx = std::task::Context::from_waker(&self.waker);
        self.doc.poll(Some(cx));

        // Process document::Style messages
        let mut has_style_changes = false;
        while let Ok(msg) = self.doc_message_rx.try_recv() {
            has_style_changes = true;
            match msg {
                DocumentMessage::CreateHeadElement {
                    name,
                    attributes,
                    contents,
                } => {
                    self.doc.create_head_element(&name, &attributes, &contents);
                }
            }
        }

        // Only do expensive work when something changed
        let needs_redraw = self.needs_redraw.swap(false, Ordering::Relaxed);
        if !needs_redraw && !has_style_changes && !hot_changed {
            return;
        }

        // Mark dirty and resolve layout
        self.doc.vdom.mark_all_dirty();
        self.doc.inner_mut().resolve(animation_time);

        // Paint + render
        self.scene.reset();
        paint_scene(
            &mut VelloScenePainter::new(&mut self.scene),
            &mut self.doc.inner_mut(),
            self.scale_factor as f64,
            self.width,
            self.height,
            0,
            0,
        );

        if self.gpu.is_offscreen() {
            if let Err(e) = self.gpu.render_offscreen(&self.scene) {
                tracing::debug!("Offscreen render error: {e}");
                return;
            }
            if let Err(e) = self.gpu.read_pixels(&mut self.readback) {
                tracing::debug!("Offscreen readback error: {e}");
                return;
            }
            self.needs_blit = true;
        } else if let Err(e) = self.gpu.render(&self.scene) {
            tracing::debug!("Embedded view render error: {e}");
        }
    }

    /// Most-recently-rendered BGRA8 pixels (offscreen mode). Returns `None`
    /// when in surface mode or no render has happened yet.
    pub fn bgra_pixels(&self) -> Option<&[u8]> {
        if self.gpu.is_offscreen() && !self.readback.is_empty() {
            Some(&self.readback)
        } else {
            None
        }
    }

    /// Returns true if a fresh offscreen frame is waiting to be blitted,
    /// and clears the flag. The consumer should call `InvalidateRect(hwnd, null, FALSE)`
    /// so SWELL posts a `WM_PAINT` that in turn calls `bgra_pixels` + `StretchBltFromMem`.
    pub fn take_needs_blit(&mut self) -> bool {
        std::mem::take(&mut self.needs_blit)
    }

    /// Mark the view as needing a redraw on the next update.
    /// Call this when external state changes that the component reads.
    pub fn mark_dirty(&self) {
        self.needs_redraw.store(true, Ordering::Relaxed);
    }

    /// Handle a resize event from the parent window.
    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);

        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.needs_redraw.store(true, Ordering::Relaxed);
            self.gpu.resize(width, height);
            let viewport = Viewport::new(width, height, self.scale_factor, ColorScheme::Dark);
            self.doc.inner_mut().set_viewport(viewport);
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The HiDPI scale this view lays out at — the divisor between the
    /// physical client pixels REAPER reports and the CSS pixels Blitz
    /// hit-tests in.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Layout size (CSS px) of the first element carrying `attr="value"`.
    ///
    /// The substitute for element resize events, which dioxus-native
    /// does not deliver (`convert_resize_data` is unimplemented): a
    /// host that needs to know how big one of its component's elements
    /// came out — the expression editor's roll, sized by flex — polls
    /// this from its own tick and pushes the answer back into app
    /// state. Read-only against the laid-out document, so it must be
    /// called from the UI thread, outside event dispatch.
    pub fn element_size_by_attr(&self, attr: &str, value: &str) -> Option<(f64, f64)> {
        let doc = self.doc.inner();
        let mut stack = vec![doc.root_element().id];
        while let Some(id) = stack.pop() {
            let Some(node) = doc.get_node(id) else {
                continue;
            };
            if let Some(el) = node.data.downcast_element()
                && el
                    .attrs()
                    .iter()
                    .any(|a| a.name.local.as_ref() == attr && a.value == value)
            {
                let rect = doc.get_client_bounding_rect(id)?;
                return Some((rect.width, rect.height));
            }
            stack.extend(node.children.iter().copied());
        }
        None
    }

    /// Forward a UI event (mouse, keyboard) to the Blitz document.
    pub fn handle_event(&mut self, event: blitz_traits::events::UiEvent) {
        self.doc.handle_ui_event(event);
        // Poll so Dioxus event handlers fire
        let cx = std::task::Context::from_waker(&self.waker);
        self.doc.poll(Some(cx));
        self.needs_redraw.store(true, Ordering::Relaxed);
    }

    /// Returns true if the currently focused node is a text-editable element
    /// (`<input type=text>`, `<textarea>`, contenteditable, etc.).
    ///
    /// Used by the REAPER `hwnd_info` hook to tell REAPER to pass typed
    /// characters through to our panel instead of processing them as global
    /// action shortcuts. Matches reaimgui's InTextField behavior (window.cpp:395-409).
    pub fn focused_is_text_input(&self) -> bool {
        let inner = self.doc.inner();
        let Some(node_id) = inner.get_focussed_node_id() else {
            return false;
        };
        let Some(node) = inner.get_node(node_id) else {
            return false;
        };
        let Some(element) = node.element_data() else {
            return false;
        };
        element.text_input_data().is_some()
    }
}

struct EmbeddedWaker(Arc<AtomicBool>);

impl ArcWake for EmbeddedWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.store(true, Ordering::Relaxed);
    }
}
