//! The winit + vello window around a [`Player`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use frame::document::proto::{NodeId, Scene};
use vello::wgpu::{self, PresentMode};
use vello::{AaConfig, Renderer, RendererOptions};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::Args;
use crate::browser::Nav;
use crate::input::{self, View};
use crate::libview::{LibraryView, Outcome};
use crate::player::{Player, banner, fonts};
use crate::source::{self, Source, Watch};

pub fn run(args: &Args) -> Result<(), String> {
    let crate::Started {
        engine,
        surface,
        error,
        library,
    } = crate::start(args)?;
    let player = Player::new(engine, surface)?;
    let now = Instant::now();
    let watch = args
        .source
        .path()
        .filter(|_| library.is_none())
        .map(|p| Watch::new(source::stamp(p), now));
    if let Some(p) = args.source.path().filter(|_| library.is_none()) {
        eprintln!("frame: watching {} for changes", p.display());
    }
    let mut app = App {
        source: args.source.clone(),
        gallery: args.gallery.clone(),
        index: 0,
        remote: args.engine.as_ref().map(|e| e.url.clone()),
        watch,
        player,
        error: None,
        banner: None,
        stats: args.stats.then(Stats::default),
        size: args.size,
        start: now,
        modifiers: ModifiersState::default(),
        cursor: None,
        context: vello::util::RenderContext::new(),
        gpu: None,
        scene: vello::Scene::new(),
        fx: args.fx,
        fx_animating: false,
        library,
        title: String::new(),
        next_poll: now,
    };
    app.set_error(error);
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop: {e}"))
}

struct Gpu {
    window: Arc<Window>,
    surface: vello::util::RenderSurface<'static>,
    renderer: Renderer,
    /// Light/shader effects around vello (see `frame::render::fx`).
    stage: frame::render::fx::Stage,
}

/// Frame-time accounting for `--stats`, printed every couple of seconds.
#[derive(Default)]
struct Stats {
    frames: u32,
    busy_ms: f64,
    since: Option<Instant>,
}

impl Stats {
    fn add(&mut self, busy: Duration, now: Instant) {
        let since = *self.since.get_or_insert(now);
        self.frames += 1;
        self.busy_ms += busy.as_secs_f64() * 1000.0;
        let wall = now - since;
        if wall >= Duration::from_secs(2) {
            eprintln!(
                "frame: {:.1} fps, avg frame {:.2} ms ({} frames)",
                f64::from(self.frames) / wall.as_secs_f64(),
                self.busy_ms / f64::from(self.frames),
                self.frames
            );
            *self = Self {
                since: Some(now),
                ..Self::default()
            };
        }
    }
}

struct App {
    source: Source,
    gallery: Vec<Source>,
    index: usize,
    /// `--engine` URL, for the title.
    remote: Option<String>,
    watch: Option<Watch>,
    player: Player,
    error: Option<String>,
    /// The banner scene (file error, else engine status), rebuilt when the
    /// message or width changes.
    banner: Option<(Scene, NodeId, f64, String)>,
    stats: Option<Stats>,
    size: (u32, u32),
    start: Instant,
    modifiers: ModifiersState,
    /// Cursor, physical window px.
    cursor: Option<(f64, f64)>,
    context: vello::util::RenderContext,
    gpu: Option<Gpu>,
    /// The banner, painted over the frame.
    scene: vello::Scene,
    fx: frame::render::fx::Mode,
    /// The last frame had a time-dependent effect (a flicker): keep drawing.
    fx_animating: bool,
    /// `--library`: the browser and the live-followed surface.
    library: Option<LibraryView>,
    /// The window title last set (library mode retitles on mode changes).
    title: String,
    /// When to next drain the library link.
    next_poll: Instant,
}

/// How often the library link is drained when nothing else redraws.
const LIBRARY_POLL: Duration = Duration::from_millis(50);

impl App {
    fn t(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn scale_factor(&self) -> f64 {
        self.gpu.as_ref().map_or(1.0, |g| g.window.scale_factor())
    }

    fn physical_size(&self) -> (f64, f64) {
        self.gpu.as_ref().map_or((1.0, 1.0), |g| {
            (
                f64::from(g.surface.config.width),
                f64::from(g.surface.config.height),
            )
        })
    }

    fn browsing(&self) -> bool {
        self.library.as_ref().is_some_and(LibraryView::browsing)
    }

    fn view(&self) -> View {
        let (w, h) = self.physical_size();
        let content = match &self.library {
            Some(lib) if lib.browsing() => lib.content(),
            _ => self.player.content(),
        };
        View::fit(content, w, h)
    }

    /// Drain the library link; redraw when something changed.
    fn poll_library(&mut self) {
        let Some(lib) = &mut self.library else { return };
        let outcomes = lib.tick(&mut self.player);
        if outcomes
            .iter()
            .any(|o| matches!(o, Outcome::Applied { .. }))
        {
            self.error = None;
            self.banner = None;
        }
        if !outcomes.is_empty() {
            self.redraw();
        }
        self.retitle();
    }

    fn retitle(&mut self) {
        let Some(lib) = &self.library else { return };
        let title = lib.title();
        if title != self.title {
            if let Some(g) = &self.gpu {
                g.window.set_title(&title);
            }
            self.title = title;
        }
    }

    /// A key for the library view; true when it took it.
    fn library_key(&mut self, key: &WinitKey) -> bool {
        let browsing = self.browsing();
        let Some(lib) = &mut self.library else {
            return false;
        };
        let ch = match key {
            WinitKey::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        match (key, ch.as_str()) {
            (WinitKey::Named(NamedKey::ArrowLeft), _) if browsing => lib.nav(Nav::Left),
            (WinitKey::Named(NamedKey::ArrowRight), _) if browsing => lib.nav(Nav::Right),
            (WinitKey::Named(NamedKey::ArrowUp), _) if browsing => lib.nav(Nav::Up),
            (WinitKey::Named(NamedKey::ArrowDown), _) if browsing => lib.nav(Nav::Down),
            (WinitKey::Named(NamedKey::PageUp), _) if browsing => lib.nav(Nav::PageUp),
            (WinitKey::Named(NamedKey::PageDown), _) if browsing => lib.nav(Nav::PageDown),
            (WinitKey::Named(NamedKey::Enter), _) if browsing => {
                lib.open_selected(&mut self.player)
            }
            (WinitKey::Named(NamedKey::Backspace), _) | (_, "b") if !browsing => lib.back(),
            (_, "i") => lib.install(),
            (_, "f") => lib.fork(&mut self.player),
            _ => return false,
        }
        self.retitle();
        true
    }

    fn redraw(&self) {
        if let Some(g) = &self.gpu {
            g.window.request_redraw();
        }
    }

    fn set_error(&mut self, error: Option<String>) {
        self.error = error;
        self.banner = None;
    }

    /// Step through the gallery (`delta` = ±1), wrapping.
    fn show(&mut self, delta: isize) {
        if self.gallery.len() < 2 {
            return;
        }
        let n = self.gallery.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(n) as usize;
        self.source = self.gallery[self.index].clone();
        self.watch = self
            .source
            .path()
            .map(|p| Watch::new(source::stamp(p), Instant::now()));
        if let (Some(g), Some(p)) = (&self.gpu, self.source.path()) {
            g.window.set_title(&format!(
                "frame — {} ({}/{})",
                p.display(),
                self.index + 1,
                self.gallery.len()
            ));
        }
        self.banner = None;
        self.reload();
    }

    fn reload(&mut self) {
        if self.library.is_some() {
            return;
        }
        let result = self
            .source
            .load(self.player.catalog())
            .and_then(|s| self.player.swap(s));
        match result {
            Ok(()) => {
                eprintln!("frame: reloaded");
                self.set_error(None);
            }
            Err(e) => {
                eprintln!("{e}");
                self.set_error(Some(e));
            }
        }
        self.redraw();
    }

    fn create_gpu(&mut self, event_loop: &ActiveEventLoop) -> Result<Gpu, String> {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_inner_size(LogicalSize::new(self.size.0, self.size.1))
                        .with_title(match (self.source.path(), &self.remote) {
                            (Some(p), _) if self.gallery.len() > 1 => {
                                format!(
                                    "frame — {} (1/{}) — ] / Tab for the next surface",
                                    p.display(),
                                    self.gallery.len()
                                )
                            }
                            (Some(p), _) => format!("frame — {}", p.display()),
                            (None, Some(url)) => format!("frame — {url}"),
                            (None, None) => "frame — demo surface".into(),
                        }),
                )
                .map_err(|e| format!("window: {e}"))?,
        );
        let size = window.inner_size();
        let surface = pollster::block_on(self.context.create_surface(
            window.clone(),
            size.width.max(1),
            size.height.max(1),
            PresentMode::AutoVsync,
        ))
        .map_err(|e| format!("gpu surface: {e}"))?;
        let renderer = Renderer::new(
            &self.context.devices[surface.dev_id].device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: [AaConfig::Area].into_iter().collect(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|e| format!("renderer: {e}"))?;
        let stage = frame::render::fx::Stage::new(&self.context.devices[surface.dev_id].device);
        Ok(Gpu {
            window,
            surface,
            renderer,
            stage,
        })
    }

    /// Cursor in surface coordinates and in logical window px.
    fn cursor(&self) -> Option<((f64, f64), (f64, f64))> {
        let (x, y) = self.cursor?;
        let s = self.scale_factor();
        Some((self.view().to_surface(x, y), (x / s, y / s)))
    }

    fn render(&mut self) {
        let started = Instant::now();
        let t = self.t();
        self.player.tick(t);
        if self.player.take_catalog_changed() && self.source.path().is_none() {
            // The engine came back different: regenerate its surface.
            eprintln!("frame: engine catalog changed; regenerating the surface");
            self.reload();
        }
        let view = self.view();
        let (width, _) = self.physical_size();
        let scale = self.scale_factor();
        let message = self
            .error
            .as_deref()
            .map(|e| self.source.short_error(e))
            .or_else(|| self.library.as_ref().and_then(LibraryView::banner))
            .or_else(|| self.player.engine_banner());
        if self
            .banner
            .as_ref()
            .is_some_and(|b| b.2 != width || Some(&b.3) != message.as_ref())
        {
            self.banner = None;
        }
        if self.banner.is_none() {
            self.banner =
                message.and_then(|m| banner(&m, width / scale).map(|(s, p)| (s, p, width, m)));
        }

        self.scene.reset();
        if let Some((b, page, _, _)) = &self.banner {
            frame::render::paint_scene_into(
                &mut self.scene,
                b,
                *page,
                kurbo::Affine::scale(scale),
                Some(fonts()),
            );
        }
        let (logical_w, logical_h) = (width / scale, self.physical_size().1 / scale);
        let toast = self
            .library
            .as_mut()
            .and_then(|lib| lib.toast(t, logical_w));
        if let Some((toast, page, tw, th)) = &toast {
            let at = kurbo::Affine::scale(scale)
                * kurbo::Affine::translate(((logical_w - tw) / 2.0, logical_h - th - 18.0));
            frame::render::paint_scene_into(&mut self.scene, toast, *page, at, Some(fonts()));
        }
        let browsing = self.browsing();
        let (doc, page, base) = match self.library.as_mut().and_then(LibraryView::document) {
            Some((scene, page)) => (scene.clone(), *page, crate::browser::BACKGROUND),
            None => {
                let (doc, page) = self.player.document(t);
                (doc, page, self.player.background())
            }
        };
        let values = if browsing {
            Default::default()
        } else {
            self.player.control_positions()
        };
        let inputs = if browsing {
            Default::default()
        } else {
            self.player.shader_inputs()
        };
        let overlay = self.banner.is_some() || toast.is_some();

        let Some(gpu) = &mut self.gpu else { return };
        let device = &self.context.devices[gpu.surface.dev_id];
        let (w, h) = (gpu.surface.config.width, gpu.surface.config.height);
        let frame = frame::render::fx::Frame {
            transform: view.affine(),
            base,
            fonts: Some(fonts()),
            mode: self.fx,
            time_s: t / 1000.0,
            values: Some(&values),
            inputs: Some(&inputs),
            overlay: overlay.then_some(&self.scene),
            antialiasing: AaConfig::Area,
            ..frame::render::fx::Frame::new(&doc, page, w, h)
        };
        let rendered =
            match gpu
                .stage
                .render(&device.device, &device.queue, &mut gpu.renderer, &frame)
            {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("frame: render failed: {e}");
                    return;
                }
            };
        self.fx_animating = gpu.stage.time_dependent() || toast.is_some();
        let (vello::wgpu::CurrentSurfaceTexture::Success(texture)
        | vello::wgpu::CurrentSurfaceTexture::Suboptimal(texture)) =
            gpu.surface.surface.get_current_texture()
        else {
            // Outdated/lost surface (mid-resize): skip; the next frame reconfigures.
            return;
        };
        let mut encoder = device
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame blit"),
            });
        gpu.surface.blitter.copy(
            &device.device,
            &mut encoder,
            &rendered.create_view(&wgpu::TextureViewDescriptor::default()),
            &texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        device.queue.submit([encoder.finish()]);
        texture.present();
        let _ = device.device.poll(wgpu::PollType::Poll);
        if let Some(stats) = &mut self.stats {
            stats.add(started.elapsed(), Instant::now());
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        match self.create_gpu(event_loop) {
            Ok(gpu) => {
                gpu.window.request_redraw();
                self.gpu = Some(gpu);
                eprintln!(
                    "frame: drag knobs/faders (Shift = fine), wheel, double-click or Ctrl/Cmd-click to reset, \
                     arrows/Page/Home/End on the last-clicked control, Esc quits"
                );
            }
            Err(e) => {
                eprintln!("frame: {e}");
                event_loop.exit();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if let (Some(watch), Some(path)) = (&mut self.watch, self.source.path())
            && watch.due(now)
            && watch.observe(source::stamp(path), now)
        {
            self.reload();
        }
        if self.library.is_some() && now >= self.next_poll {
            self.next_poll = now + LIBRARY_POLL;
            self.poll_library();
        }
        if (self.player.animating() && !self.browsing()) || self.fx_animating {
            self.redraw();
        }
        let deadline = match (&self.watch, self.library.is_some()) {
            (Some(w), _) => Some(w.deadline()),
            (None, true) => Some(self.next_poll),
            (None, false) => None,
        };
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.gpu.as_ref().is_none_or(|g| g.window.id() != id) {
            return;
        }
        let t = self.t();
        let mods = input::modifiers(self.modifiers);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => return self.render(),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.gpu {
                    self.context.resize_surface(
                        &mut g.surface,
                        size.width.max(1),
                        size.height.max(1),
                    );
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => self.banner = None,
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Focused(false) => self.player.cancel(t),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                if self.browsing() {
                    return;
                }
                if let Some((at, _)) = self.cursor() {
                    self.player.pointer_move(at, mods, t);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    let cursor = self.cursor();
                    if let (Some(lib), Some((at, _))) =
                        (self.library.as_mut().filter(|l| l.browsing()), cursor)
                    {
                        lib.click(at, &mut self.player);
                        self.retitle();
                    } else if let Some((at, logical)) = cursor {
                        self.player.pointer_down(at, logical, mods, t);
                    }
                }
                ElementState::Released => self.player.pointer_up(t),
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = input::wheel_notches(delta, self.scale_factor());
                if let Some(lib) = self.library.as_mut().filter(|l| l.browsing()) {
                    lib.nav(if notches > 0.0 { Nav::Up } else { Nav::Down });
                } else {
                    self.player.wheel(notches, mods, t);
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                if let (false, Some((at, _))) = (self.browsing(), self.cursor()) {
                    self.player.context(at, t);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.player.text(&text);
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && self.player.editing() =>
            {
                // A field being edited takes every key: named ones as keys,
                // the rest as the text they type.
                let key = input::key(&event.logical_key)
                    .filter(|k| !matches!(k, frame::surface::runtime::Key::Char(_)) || mods.alt);
                match key {
                    Some(k) => {
                        self.player.key(k, mods, t);
                    }
                    None => {
                        if let Some(text) = &event.text {
                            self.player.text(text);
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let shift = self.modifiers.shift_key();
                let taken = |k: &WinitKey, p: &mut crate::player::Player| {
                    input::key(k).is_some_and(|key| p.key(key, mods, t))
                };
                if event.logical_key == WinitKey::Named(NamedKey::Escape) {
                    // Escape closes a menu or clears the selection first.
                    if !taken(&event.logical_key, &mut self.player) {
                        event_loop.exit();
                    }
                } else if self.library_key(&event.logical_key) {
                } else if event.logical_key == WinitKey::Named(NamedKey::Tab) {
                    self.show(if shift { -1 } else { 1 });
                } else if event.logical_key == WinitKey::Character("]".into()) {
                    self.show(1);
                } else if event.logical_key == WinitKey::Character("[".into()) {
                    self.show(-1);
                } else if let Some(key) = input::key(&event.logical_key) {
                    self.player.key(key, mods, t);
                }
                // IME follows text editing (a click into a name field).
                if let Some(g) = &self.gpu {
                    g.window.set_ime_allowed(self.player.editing());
                }
            }
            _ => return,
        }
        self.redraw();
    }
}
