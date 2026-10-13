#[cfg(feature = "anyrender")]
use frame::document::proto::{NodeId, Scene as Document};
#[cfg(feature = "anyrender")]
use frame::render::build_scene_layout_boxes;
#[cfg(feature = "anyrender")]
use kurbo::{Affine, Point, Rect};
#[cfg(feature = "anyrender")]
use peniko::{Blob, Brush, BrushRef, Color, Fill, FontData, color::palette};
#[cfg(feature = "anyrender")]
use skrifa::{
    MetadataProvider,
    raw::{FileRef, FontRef},
};
#[cfg(feature = "anyrender")]
use std::collections::VecDeque;
#[cfg(feature = "anyrender")]
use std::sync::Arc;
#[cfg(feature = "anyrender")]
use std::time::Instant;
#[cfg(feature = "anyrender")]
use vello::wgpu::{self, PresentMode};
#[cfg(feature = "anyrender")]
use vello::{AaConfig, Glyph, RenderParams, Renderer, RendererOptions, Scene};
#[cfg(feature = "anyrender")]
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("wgpu_preview requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(err) = run() {
        eprintln!("wgpu_preview error: {err}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
const SLIDING_WINDOW_SIZE: usize = 100;
#[cfg(feature = "anyrender")]
const AA_CONFIGS: [AaConfig; 3] = [AaConfig::Area, AaConfig::Msaa8, AaConfig::Msaa16];
#[cfg(feature = "anyrender")]
const ROBOTO_FONT: &[u8] = include_bytes!("../../assets/roboto/Roboto-Regular.ttf");

#[cfg(feature = "anyrender")]
struct SimpleText {
    roboto: FontData,
}

#[cfg(feature = "anyrender")]
impl SimpleText {
    fn new() -> Self {
        Self {
            roboto: FontData::new(Blob::new(Arc::new(ROBOTO_FONT)), 0),
        }
    }

    fn add<'a>(
        &mut self,
        scene: &mut Scene,
        size: f32,
        brush: impl Into<BrushRef<'a>>,
        transform: Affine,
        text: &str,
    ) {
        let font_ref = to_font_ref(&self.roboto).unwrap();
        let brush = brush.into();
        let axes = font_ref.axes();
        let var_loc = axes.location(std::iter::empty::<(&str, f32)>());
        let font_size = skrifa::instance::Size::new(size);
        let charmap = font_ref.charmap();
        let metrics = font_ref.metrics(font_size, &var_loc);
        let line_height = metrics.ascent - metrics.descent + metrics.leading;
        let glyph_metrics = font_ref.glyph_metrics(font_size, &var_loc);
        let mut pen_x = 0.0_f32;
        let mut pen_y = 0.0_f32;

        scene
            .draw_glyphs(&self.roboto)
            .font_size(size)
            .transform(transform)
            .brush(brush)
            .draw(
                Fill::NonZero,
                text.chars().filter_map(|ch| {
                    if ch == '\n' {
                        pen_y += line_height;
                        pen_x = 0.0;
                        return None;
                    }
                    let gid = charmap.map(ch).unwrap_or_default();
                    let advance = glyph_metrics.advance_width(gid).unwrap_or_default();
                    let x = pen_x;
                    pen_x += advance;
                    Some(Glyph {
                        id: gid.to_u32(),
                        x,
                        y: pen_y,
                    })
                }),
            );
    }
}

#[cfg(feature = "anyrender")]
fn to_font_ref(font: &FontData) -> Option<FontRef<'_>> {
    let file_ref = FileRef::new(font.data.as_ref()).ok()?;
    match file_ref {
        FileRef::Font(font) => Some(font),
        FileRef::Collection(collection) => collection.get(font.index).ok(),
    }
}

#[cfg(feature = "anyrender")]
#[derive(Debug)]
struct Snapshot {
    fps: f64,
    frame_time_ms: f64,
    frame_time_min_ms: f64,
    frame_time_max_ms: f64,
}

#[cfg(feature = "anyrender")]
impl Snapshot {
    fn draw_layer<'a, T>(
        &self,
        scene: &mut Scene,
        text: &mut SimpleText,
        viewport_width: f64,
        viewport_height: f64,
        samples: T,
        vsync: bool,
        aa_config: AaConfig,
    ) where
        T: Iterator<Item = &'a u64>,
    {
        let width = (viewport_width * 0.4).clamp(200.0, 600.0);
        let height = width * 0.7;
        let x_offset = viewport_width - width;
        let y_offset = viewport_height - height;
        let offset = Affine::translate((x_offset, y_offset));

        scene.fill(
            Fill::NonZero,
            offset,
            palette::css::BLACK.with_alpha(0.75),
            None,
            &Rect::new(0.0, 0.0, width, height),
        );

        let labels = vec![
            format!("Frame Time: {:.2} ms", self.frame_time_ms),
            format!("Frame Time (min): {:.2} ms", self.frame_time_min_ms),
            format!("Frame Time (max): {:.2} ms", self.frame_time_max_ms),
            format!("VSync: {}", if vsync { "on" } else { "off" }),
            format!(
                "AA method: {}",
                match aa_config {
                    AaConfig::Area => "Analytic Area",
                    AaConfig::Msaa16 => "16xMSAA",
                    AaConfig::Msaa8 => "8xMSAA",
                }
            ),
            format!("Resolution: {viewport_width}x{viewport_height}"),
        ];

        let text_height = height * 0.5 / (1 + labels.len()) as f64;
        let left_margin = width * 0.01;
        let text_size = (text_height * 0.9) as f32;
        for (i, label) in labels.iter().enumerate() {
            text.add(
                scene,
                text_size,
                Brush::Solid(palette::css::WHITE),
                offset * Affine::translate((left_margin, (i + 1) as f64 * text_height)),
                label,
            );
        }
        text.add(
            scene,
            text_size,
            Brush::Solid(palette::css::WHITE),
            offset * Affine::translate((width * 0.67, text_height)),
            &format!("FPS: {:.2}", self.fps),
        );

        use kurbo::PathEl;
        let left_padding = width * 0.05;
        let graph_max_height = height * 0.5;
        let graph_max_width = width - 2.0 * left_margin - left_padding;
        let left_margin_padding = left_margin + left_padding;
        let bar_extent = graph_max_width / (SLIDING_WINDOW_SIZE as f64);
        let bar_width = bar_extent * 0.4;
        let bar = [
            PathEl::MoveTo((0.0, graph_max_height).into()),
            PathEl::LineTo((0.0, 0.0).into()),
            PathEl::LineTo((bar_width, 0.0).into()),
            PathEl::LineTo((bar_width, graph_max_height).into()),
        ];

        let display_max = if self.frame_time_max_ms > 3.0 * self.frame_time_ms {
            round_up((1.33334 * self.frame_time_ms) as usize, 5) as f64
        } else {
            self.frame_time_max_ms
        };
        for (i, sample) in samples.enumerate() {
            let t = offset * Affine::translate((i as f64 * bar_extent, graph_max_height));
            let sample_ms = ((*sample as f64) * 0.001).min(display_max);
            let h = sample_ms / display_max;
            let s = Affine::scale_non_uniform(1.0, -h);
            let color = match *sample {
                ..=16_667 => Color::from_rgb8(100, 143, 255),
                ..=33_334 => Color::from_rgb8(255, 176, 0),
                _ => Color::from_rgb8(220, 38, 127),
            };
            scene.fill(
                Fill::NonZero,
                t * Affine::translate((
                    left_margin_padding,
                    (1 + labels.len()) as f64 * text_height,
                )) * s,
                color,
                None,
                &bar,
            );
        }

        let marker = [
            PathEl::MoveTo((0.0, graph_max_height).into()),
            PathEl::LineTo((graph_max_width, graph_max_height).into()),
        ];
        let thresholds = [8.33, 16.66, 33.33];
        let thres_text_height = graph_max_height * 0.05;
        let thres_text_height_2 = thres_text_height * 0.5;
        for t in thresholds.iter().filter(|&&t| t < display_max) {
            let y = t / display_max;
            text.add(
                scene,
                thres_text_height as f32,
                Brush::Solid(palette::css::WHITE),
                offset
                    * Affine::translate((
                        left_margin,
                        (2.0 - y) * graph_max_height + thres_text_height_2,
                    )),
                &format!("{t}"),
            );
            scene.stroke(
                &kurbo::Stroke::new(graph_max_height * 0.01),
                offset * Affine::translate((left_margin_padding, (1.0 - y) * graph_max_height)),
                palette::css::WHITE,
                None,
                &marker,
            );
        }
    }
}

#[cfg(feature = "anyrender")]
struct Sample {
    frame_time_us: u64,
}

#[cfg(feature = "anyrender")]
struct Stats {
    count: usize,
    sum: u64,
    min: u64,
    max: u64,
    samples: VecDeque<u64>,
}

#[cfg(feature = "anyrender")]
impl Stats {
    fn new() -> Self {
        Self {
            count: 0,
            sum: 0,
            min: u64::MAX,
            max: u64::MIN,
            samples: VecDeque::with_capacity(SLIDING_WINDOW_SIZE),
        }
    }

    fn add_sample(&mut self, sample: Sample) {
        let oldest = if self.count < SLIDING_WINDOW_SIZE {
            self.count += 1;
            None
        } else {
            self.samples.pop_front()
        };

        let micros = sample.frame_time_us;
        self.sum += micros;
        self.samples.push_back(micros);
        if let Some(oldest) = oldest {
            self.sum -= oldest;
        }
        self.min = self.min.min(micros);
        self.max = self.max.max(micros);
    }

    fn clear_min_and_max(&mut self) {
        self.min = u64::MAX;
        self.max = u64::MIN;
    }

    fn samples(&self) -> impl Iterator<Item = &u64> {
        self.samples.iter()
    }

    fn snapshot(&self) -> Option<Snapshot> {
        if self.count == 0 || self.sum == 0 {
            return None;
        }
        let frame_time_ms = (self.sum as f64 / self.count as f64) * 0.001;
        let fps = 1000.0 / frame_time_ms;
        Some(Snapshot {
            fps,
            frame_time_ms,
            frame_time_min_ms: self.min as f64 * 0.001,
            frame_time_max_ms: self.max as f64 * 0.001,
        })
    }
}

#[cfg(feature = "anyrender")]
fn draw_overlay(
    scene: &mut Scene,
    stats: &Stats,
    text: &mut SimpleText,
    width: f64,
    height: f64,
    vsync: bool,
    aa_config: AaConfig,
) {
    let Some(snapshot) = stats.snapshot() else {
        return;
    };
    snapshot.draw_layer(
        scene,
        text,
        width,
        height,
        stats.samples(),
        vsync,
        aa_config,
    );
}

#[cfg(feature = "anyrender")]
fn round_up(n: usize, f: usize) -> usize {
    n - 1 - (n - 1) % f + f
}

#[cfg(feature = "anyrender")]
struct App {
    doc: Arc<Document>,
    root: NodeId,
    base_transform: Affine,
    transform: Affine,
    mouse_down: bool,
    prior_position: Option<Point>,
    width: u32,
    height: u32,
    title: String,
    aa_config_ix: i32,
    vsync_on: bool,
    stats: Stats,
    stats_shown: bool,
    modifiers: ModifiersState,
    frame_start_time: Instant,
    simple_text: SimpleText,

    context: vello::util::RenderContext,
    renderer: Option<Renderer>,
    surface: Option<vello::util::RenderSurface<'static>>,
    window: Option<Arc<Window>>,
    scene: Scene,
}

#[cfg(feature = "anyrender")]
impl App {
    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn create_or_resume_surface(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_some() {
            return;
        }
        let window = self.window.take().unwrap_or_else(|| {
            Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_inner_size(LogicalSize::new(self.width, self.height))
                            .with_resizable(true)
                            .with_title(self.title.clone())
                            .with_visible(true)
                            .with_active(true),
                    )
                    .expect("window creation failed"),
            )
        });

        let size = window.inner_size();
        self.width = size.width.max(1);
        self.height = size.height.max(1);

        let present_mode = if self.vsync_on {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        };

        let surface = pollster::block_on(self.context.create_surface(
            window.clone(),
            self.width,
            self.height,
            present_mode,
        ))
        .expect("Error creating surface");

        let dev_id = surface.dev_id;
        let device = &self.context.devices[dev_id].device;
        let renderer = Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AA_CONFIGS.iter().copied().collect(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .expect("Failed to create renderer");

        self.renderer = Some(renderer);
        self.surface = Some(surface);
        self.window = Some(window.clone());

        eprintln!(
            "wgpu_preview: window created id={:?} size={}x{}",
            window.id(),
            self.width,
            self.height
        );
        eprintln!(
            "controls: left-drag=pan, wheel=zoom, space=reset transform, q/e=rotate, m=cycle AA (shift=reverse), v=toggle vsync, s=toggle stats, c=clear min/max"
        );
        self.request_redraw();
    }

    fn aa_config(&self) -> AaConfig {
        AA_CONFIGS[self.aa_config_ix.rem_euclid(AA_CONFIGS.len() as i32) as usize]
    }

    fn toggle_vsync(&mut self) {
        self.vsync_on = !self.vsync_on;
        if let Some(surface) = &mut self.surface {
            self.context.set_present_mode(
                surface,
                if self.vsync_on {
                    PresentMode::AutoVsync
                } else {
                    PresentMode::AutoNoVsync
                },
            );
        }
    }
}

#[cfg(feature = "anyrender")]
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.create_or_resume_surface(event_loop);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        let _ = self.surface.take();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = &self.window else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match event.logical_key.as_ref() {
                        Key::Named(NamedKey::Space) => {
                            self.transform = Affine::IDENTITY;
                        }
                        Key::Named(NamedKey::Escape) => event_loop.exit(),
                        Key::Character(char) => {
                            let char = char.to_lowercase();
                            match char.as_str() {
                                "q" | "e" => {
                                    if let Some(prior_position) = self.prior_position {
                                        let is_clockwise = char == "e";
                                        let angle = if is_clockwise { -0.05 } else { 0.05 };
                                        self.transform =
                                            self.transform.then_rotate_about(angle, prior_position);
                                    }
                                }
                                "s" => self.stats_shown = !self.stats_shown,
                                "c" => self.stats.clear_min_and_max(),
                                "m" => {
                                    self.aa_config_ix = if self.modifiers.shift_key() {
                                        self.aa_config_ix.saturating_sub(1)
                                    } else {
                                        self.aa_config_ix.saturating_add(1)
                                    };
                                }
                                "v" => self.toggle_vsync(),
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::Resized(size) => {
                self.width = size.width.max(1);
                self.height = size.height.max(1);
                if let Some(surface) = &mut self.surface {
                    self.context
                        .resize_surface(surface, self.width, self.height);
                }
                self.request_redraw();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    self.mouse_down = state == ElementState::Pressed;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                const BASE: f64 = 1.05;
                const PIXELS_PER_LINE: f64 = 20.0;

                if let Some(prior_position) = self.prior_position {
                    let exponent = if let MouseScrollDelta::PixelDelta(delta) = delta {
                        delta.y / PIXELS_PER_LINE
                    } else if let MouseScrollDelta::LineDelta(_, y) = delta {
                        y as f64
                    } else {
                        0.0
                    };
                    self.transform = self
                        .transform
                        .then_scale_about(BASE.powf(exponent), prior_position);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.prior_position = None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = Point::new(position.x, position.y);
                if self.mouse_down {
                    if let Some(prior) = self.prior_position {
                        self.transform = self.transform.then_translate(position - prior);
                    }
                }
                self.prior_position = Some(position);
            }
            WindowEvent::RedrawRequested => {
                let aa = self.aa_config();
                let (Some(surface), Some(renderer), Some(window)) =
                    (&mut self.surface, &mut self.renderer, &self.window)
                else {
                    return;
                };

                window.request_redraw();

                let width = surface.config.width;
                let height = surface.config.height;
                let device_handle = &self.context.devices[surface.dev_id];

                self.scene.reset();
                let scene_transform = self.transform * self.base_transform;
                frame::render::paint_scene_into(
                    &mut self.scene,
                    self.doc.as_ref(),
                    self.root,
                    scene_transform,
                    None,
                );
                if self.stats_shown {
                    draw_overlay(
                        &mut self.scene,
                        &self.stats,
                        &mut self.simple_text,
                        width as f64,
                        height as f64,
                        self.vsync_on,
                        aa,
                    );
                }

                renderer
                    .render_to_texture(
                        &device_handle.device,
                        &device_handle.queue,
                        &self.scene,
                        &surface.target_view,
                        &RenderParams {
                            base_color: Color::WHITE,
                            width,
                            height,
                            antialiasing_method: aa,
                        },
                    )
                    .expect("failed to render to texture");

                let surface_texture = surface.surface.get_current_texture();
                let surface_texture = match surface_texture {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    other => panic!("failed to get surface texture: {other:?}"),
                };
                let mut encoder =
                    device_handle
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Surface Blit"),
                        });
                surface.blitter.copy(
                    &device_handle.device,
                    &mut encoder,
                    &surface.target_view,
                    &surface_texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                );
                device_handle.queue.submit([encoder.finish()]);
                surface_texture.present();
                let _ = device_handle.device.poll(wgpu::PollType::Poll);

                let now = Instant::now();
                self.stats.add_sample(Sample {
                    frame_time_us: (now - self.frame_start_time).as_micros() as u64,
                });
                self.frame_start_time = now;

                if let Some(snapshot) = self.stats.snapshot() {
                    window.set_title(&format!(
                        "{} | FPS {:.1} | avg {:.2}ms | min {:.2}ms | max {:.2}ms | AA {:?} | vsync {}",
                        self.title,
                        snapshot.fps,
                        snapshot.frame_time_ms,
                        snapshot.frame_time_min_ms,
                        snapshot.frame_time_max_ms,
                        aa,
                        if self.vsync_on { "on" } else { "off" },
                    ));
                }
            }
            _ => {}
        }
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or_else(|| {
        "usage: wgpu_preview <input_json> [--root FIGMA_ID] [--width PX] [--height PX] [--title TEXT]".to_string()
    })?;

    let mut root_figma_id: Option<String> = None;
    let mut width_override: Option<u32> = None;
    let mut height_override: Option<u32> = None;
    let mut title = "Frame WGPU Preview".to_string();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root_figma_id = args.next(),
            "--width" => {
                width_override = Some(
                    args.next()
                        .ok_or_else(|| "--width requires value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --width value: {e}"))?,
                )
            }
            "--height" => {
                height_override = Some(
                    args.next()
                        .ok_or_else(|| "--height requires value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --height value: {e}"))?,
                )
            }
            "--title" => {
                title = args
                    .next()
                    .ok_or_else(|| "--title requires value".to_string())?;
            }
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    let bytes = std::fs::read(&input).map_err(|e| format!("failed reading {input}: {e}"))?;
    let doc = frame::figma::import::import_bytes(&bytes)
        .map_err(|e| format!("failed importing source document from {input}: {e}"))?;
    let root = match root_figma_id {
        Some(figma_id) => frame::figma::import::node_for_figma_id(&doc, &figma_id)
            .ok_or_else(|| format!("root figma id not found: {figma_id}"))?,
        None => doc.pages().first().copied().unwrap_or(doc.root),
    };

    let layout_boxes = build_scene_layout_boxes(&doc, root);
    let subtree = doc.walk(root);

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for node_id in subtree {
        if let Some(b) = layout_boxes.get(&node_id).copied() {
            if b.width <= 0.0 || b.height <= 0.0 {
                continue;
            }
            min_x = min_x.min(b.x);
            min_y = min_y.min(b.y);
            max_x = max_x.max(b.x + b.width);
            max_y = max_y.max(b.y + b.height);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 1.0;
        max_y = 1.0;
    }

    let natural_width = (max_x - min_x).ceil().max(1.0) as u32;
    let natural_height = (max_y - min_y).ceil().max(1.0) as u32;
    let default_width = natural_width.max(1280);
    let default_height = natural_height.max(720);
    let width = width_override.unwrap_or(default_width).clamp(64, 4096);
    let height = height_override.unwrap_or(default_height).clamp(64, 4096);

    eprintln!(
        "wgpu_preview: root={} natural={}x{} window={}x{}",
        doc.node(root)
            .and_then(|n| n.source.as_ref())
            .map(|s| s.id.as_str())
            .unwrap_or("<unknown>"),
        natural_width,
        natural_height,
        width,
        height
    );

    let mut app = App {
        doc: Arc::new(doc),
        root,
        base_transform: Affine::translate((-min_x, -min_y)),
        transform: Affine::IDENTITY,
        mouse_down: false,
        prior_position: None,
        width,
        height,
        title,
        aa_config_ix: 0,
        vsync_on: true,
        stats: Stats::new(),
        stats_shown: true,
        modifiers: ModifiersState::default(),
        frame_start_time: Instant::now(),
        simple_text: SimpleText::new(),
        context: vello::util::RenderContext::new(),
        renderer: None,
        surface: None,
        window: None,
        scene: Scene::new(),
    };

    let event_loop = EventLoop::new().map_err(|e| format!("failed creating event loop: {e}"))?;
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("failed running app: {e}"))
}
