//! Vello Chart Renderer Module
//!
//! Renders charts using Vello with WebGPU backend.
//! Ported from apps/web/src/renderer.rs for the documentation site.

use anyrender_vello::VelloScenePainter;
use keyflow::Chart;
use keyflow::engraver::fonts::ChartFontBundle;
use keyflow::engraver::layout::ChartLayoutMode;
use keyflow::engraver::layout::chart::{ChartLayoutConfig, ChartLayoutEngine, ChartLayoutResult};
use keyflow::engraver::renderer::scene_renderer::SceneRenderBuilder;
use keyflow::engraver::style::MStyle;
use vello::Scene;
use vello::kurbo::{Affine, Rect};
use vello::peniko::Color;

// Fonts are now provided by ChartFontBundle from keyflow

/// Screen DPI for rendering
const SCREEN_DPI: f64 = 96.0;
/// Points per inch (typographical standard)
const POINTS_PER_INCH: f64 = 72.0;
/// DPI scaling factor: converts points to screen pixels
const DPI_SCALE: f64 = SCREEN_DPI / POINTS_PER_INCH;

/// Chart layout and rendering engine for web.
///
/// Manages fonts, layout engine, and Vello scene rendering.
pub struct ChartLayoutManager {
    /// Font bundle (single source of truth for all chart fonts)
    font_bundle: ChartFontBundle,
    /// Layout engine
    layout_engine: ChartLayoutEngine,
    /// Cached layout result
    layout_result: Option<ChartLayoutResult>,
    /// Last rendered chart hash (for cache invalidation)
    last_chart_hash: u64,
    /// WebGPU renderer (WASM only)
    #[cfg(target_arch = "wasm32")]
    wgpu_renderer: Option<wasm::WebGpuRenderer>,
}

impl ChartLayoutManager {
    /// Create a new chart layout manager with embedded fonts.
    pub fn new() -> Result<Self, String> {
        // Load font bundle (single source of truth)
        let font_bundle = ChartFontBundle::new()?;

        // Create layout engine with correct font wiring via bundle
        let style = Box::leak(Box::new(MStyle::new()));
        let layout_engine = font_bundle.create_layout_engine(style);

        Ok(Self {
            font_bundle,
            layout_engine,
            layout_result: None,
            last_chart_hash: 0,
            #[cfg(target_arch = "wasm32")]
            wgpu_renderer: None,
        })
    }

    /// Layout a chart using snippet mode (content-sized, no fixed page).
    pub fn layout_chart(&mut self, chart: &Chart, viewport_width: f64, _viewport_height: f64) {
        self.layout_chart_with_mode(chart, viewport_width, true);
    }

    /// Layout a chart with a specified mode.
    ///
    /// # Arguments
    /// * `chart` - The parsed chart to layout
    /// * `viewport_width` - Width of the viewport in CSS pixels
    /// * `snippet_mode` - If true, use snippet mode (content-sized). If false, use A4 paginated mode.
    pub fn layout_chart_with_mode(
        &mut self,
        chart: &Chart,
        viewport_width: f64,
        snippet_mode: bool,
    ) {
        self.layout_chart_with_options(chart, viewport_width, snippet_mode, true);
    }

    /// Layout a chart for export (no page offsets, positioned at origin).
    ///
    /// # Arguments
    /// * `chart` - The parsed chart to layout
    /// * `viewport_width` - Width of the viewport in CSS pixels
    /// * `snippet_mode` - If true, use snippet mode (content-sized). If false, use A4 paginated mode.
    pub fn layout_chart_for_export(
        &mut self,
        chart: &Chart,
        viewport_width: f64,
        snippet_mode: bool,
    ) {
        self.layout_chart_with_options(chart, viewport_width, snippet_mode, false);
    }

    /// Layout a chart with full control over options.
    ///
    /// # Arguments
    /// * `chart` - The parsed chart to layout
    /// * `viewport_width` - Width of the viewport in CSS pixels
    /// * `snippet_mode` - If true, use snippet mode (content-sized). If false, use A4 paginated mode.
    /// * `use_page_offsets` - If true, add 20pt offset for multi-page viewing. If false, position at origin.
    fn layout_chart_with_options(
        &mut self,
        chart: &Chart,
        viewport_width: f64,
        snippet_mode: bool,
        use_page_offsets: bool,
    ) {
        // Simple hash based on chart data, mode, and offset setting
        let chart_hash =
            self.compute_chart_hash_with_options(chart, snippet_mode, use_page_offsets);

        // Skip if already laid out
        if self.layout_result.is_some() && chart_hash == self.last_chart_hash {
            return;
        }

        // A4 dimensions in points (72 points per inch)
        const A4_WIDTH: f64 = 595.0; // 210mm = 8.27" = 595 points
        const A4_HEIGHT: f64 = 842.0; // 297mm = 11.69" = 842 points

        let (mode, config) = if snippet_mode {
            // Snippet mode: content-sized, minimal margins
            let config = ChartLayoutConfig::snippet().with_page_offsets(use_page_offsets);
            let mode = ChartLayoutMode::Snippet {
                page_width: viewport_width / DPI_SCALE,
            };
            (mode, config)
        } else {
            // Page mode: A4 paginated with Master Rhythm preset
            let config = ChartLayoutConfig::master_rhythm().with_page_offsets(use_page_offsets);
            let mode = ChartLayoutMode::Paginated {
                page_width: A4_WIDTH,
                page_height: A4_HEIGHT,
            };
            (mode, config)
        };

        // Perform layout
        let result = self
            .layout_engine
            .layout_chart_with_config(chart, &mode, &config);

        self.layout_result = Some(result);
        self.last_chart_hash = chart_hash;
    }

    /// Compute a hash of the chart including layout mode (legacy, for backward compatibility).
    #[allow(dead_code)]
    fn compute_chart_hash_with_mode(&self, chart: &Chart, snippet_mode: bool) -> u64 {
        self.compute_chart_hash_with_options(chart, snippet_mode, true)
    }

    /// Compute a hash of the chart including all layout options.
    ///
    /// This hash is used for cache invalidation - if the hash changes, we re-layout.
    /// We hash the debug representation of the chart which includes all content.
    fn compute_chart_hash_with_options(
        &self,
        chart: &Chart,
        snippet_mode: bool,
        use_page_offsets: bool,
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        // Hash key chart fields for cache invalidation.
        // Chart doesn't derive Debug, so we hash its accessible sub-fields.
        format!("{:?}", chart.metadata).hash(&mut hasher);
        chart.sections.len().hash(&mut hasher);
        format!("{:?}", chart.tempo).hash(&mut hasher);
        format!("{:?}", chart.time_signature).hash(&mut hasher);
        format!("{:?}", chart.key_changes).hash(&mut hasher);
        format!("{:?}", chart.tempo_changes).hash(&mut hasher);
        format!("{:?}", chart.settings).hash(&mut hasher);
        // Hash each section's debug output for content changes
        for section in &chart.sections {
            format!("{:?}", section).hash(&mut hasher);
        }
        snippet_mode.hash(&mut hasher);
        use_page_offsets.hash(&mut hasher);

        hasher.finish()
    }

    /// Get the cached layout result.
    pub fn layout_result(&self) -> Option<&ChartLayoutResult> {
        self.layout_result.as_ref()
    }

    /// Render the chart to a Vello scene.
    pub fn render_to_scene(&self, scene: &mut Scene, width: f64, height: f64, transform: Affine) {
        // Fill background
        scene.fill(
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(55, 65, 81), // gray-700
            None,
            &Rect::new(0.0, 0.0, width, height),
        );

        // Render layout if available
        if let Some(ref layout) = self.layout_result {
            // Create scene renderer with fonts
            let base_renderer = SceneRenderBuilder::new().spatium(5.0).build();
            let mut renderer = self.font_bundle.configure_renderer(base_renderer);

            // Set viewport for culling
            let viewport_rect = Rect::new(0.0, 0.0, width, height);
            renderer.set_viewport(viewport_rect);

            // Render with user's view transform
            renderer.render_with_transform(
                &mut VelloScenePainter::new(scene),
                &layout.scene,
                transform,
            );
        }
    }

    /// Render to a canvas element (WASM only).
    #[cfg(target_arch = "wasm32")]
    pub async fn render_to_canvas(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions first
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed (access renderer mutably)
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create scene (borrow self immutably for rendering data)
        let mut scene = Scene::new();
        let transform = Affine::translate((20.0, 20.0)) * Affine::scale(DPI_SCALE);

        // Fill background
        scene.fill(
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(55, 65, 81),
            None,
            &Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64),
        );

        // Render layout if available
        if let Some(ref layout) = self.layout_result {
            // Create scene renderer with fonts
            let base_renderer = SceneRenderBuilder::new().spatium(5.0).build();
            let mut renderer = self.font_bundle.configure_renderer(base_renderer);

            // Set viewport for culling
            let viewport_rect = Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64);
            renderer.set_viewport(viewport_rect);

            // Render with user's view transform
            renderer.render_with_transform(
                &mut VelloScenePainter::new(&mut scene),
                &layout.scene,
                transform,
            );
        }

        // Render to canvas (access wgpu_renderer mutably again)
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render(&scene)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Render to a canvas element with custom transform (WASM only).
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to render to
    /// * `translate_x` - X translation in pixels (already scaled by DPR)
    /// * `translate_y` - Y translation in pixels (already scaled by DPR)
    /// * `scale` - Scale factor (already includes DPR)
    #[cfg(target_arch = "wasm32")]
    pub async fn render_to_canvas_with_transform(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
        translate_x: f64,
        translate_y: f64,
        scale: f64,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions first
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed (access renderer mutably)
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create scene (borrow self immutably for rendering data)
        let mut scene = Scene::new();

        // Build transform: translate then scale
        let transform = Affine::translate((translate_x, translate_y)) * Affine::scale(scale);

        // Fill background
        scene.fill(
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(55, 65, 81),
            None,
            &Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64),
        );

        // Render layout if available
        if let Some(ref layout) = self.layout_result {
            // Create scene renderer with fonts
            let base_renderer = SceneRenderBuilder::new().spatium(5.0).build();
            let mut renderer = self.font_bundle.configure_renderer(base_renderer);

            // Set viewport for culling
            let viewport_rect = Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64);
            renderer.set_viewport(viewport_rect);

            // Render with user's view transform
            renderer.render_with_transform(
                &mut VelloScenePainter::new(&mut scene),
                &layout.scene,
                transform,
            );
        }

        // Render to canvas (access wgpu_renderer mutably again)
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render(&scene)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Render to a canvas element without background (WASM only).
    ///
    /// This renders ONLY the content (the white page) with no gray workspace background.
    /// Ideal for showcases and embedded previews where the page should be edge-to-edge.
    /// The page is rendered at canvas origin (0,0) with proper DPI and device pixel ratio scaling.
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to render to
    /// * `dpr` - Device pixel ratio for high-DPI displays
    #[cfg(target_arch = "wasm32")]
    pub async fn render_to_canvas_transparent(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
        dpr: f64,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions first
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed (access renderer mutably)
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create scene WITHOUT background fill
        let mut scene = Scene::new();

        // Get page dimensions from layout to calculate fit-to-width scale
        let (page_width_pt, content_x, content_y) = self
            .layout_result
            .as_ref()
            .map(|layout| {
                let bounds = layout.scene.compute_bounds();
                // Page width is the total width from the layout
                let page_width = layout.total_width;
                // Use max with 0 to ignore negative bounds (elements extending outside page)
                (page_width, bounds.x0.max(0.0), bounds.y0.max(0.0))
            })
            .unwrap_or((595.0, 0.0, 0.0)); // Default to A4 width

        // Calculate scale to fit page width to canvas width
        // canvas_width is in buffer pixels, so divide by dpr to get CSS pixels
        let canvas_css_width = canvas_width as f64 / dpr;
        let page_css_width = page_width_pt * DPI_SCALE;
        let fit_scale = canvas_css_width / page_css_width;

        // Combined scale: fit_scale (to fill container) * DPI_SCALE (points to CSS) * dpr (CSS to buffer)
        let total_scale = fit_scale * DPI_SCALE * dpr;

        // Build transform: offset to bring page to origin, then scale
        let transform = Affine::translate((-content_x * total_scale, -content_y * total_scale))
            * Affine::scale(total_scale);

        // Render layout if available - NO background fill
        if let Some(ref layout) = self.layout_result {
            // Create scene renderer with fonts
            let base_renderer = SceneRenderBuilder::new().spatium(5.0).build();
            let mut renderer = self.font_bundle.configure_renderer(base_renderer);

            // Set viewport for culling
            let viewport_rect = Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64);
            renderer.set_viewport(viewport_rect);

            // Render with transform
            renderer.render_with_transform(
                &mut VelloScenePainter::new(&mut scene),
                &layout.scene,
                transform,
            );
        }

        // Render to canvas with TRANSPARENT base color (only page content shows)
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render_with_base_color(&scene, Color::TRANSPARENT)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Render to a canvas element with white base color (WASM only).
    ///
    /// Similar to `render_to_canvas_transparent` but uses white background.
    /// This prevents flashing during transitions (e.g., typewriter animation)
    /// by ensuring the base is always white, matching the page color.
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to render to
    /// * `dpr` - Device pixel ratio for high-DPI displays
    #[cfg(target_arch = "wasm32")]
    pub async fn render_to_canvas_white(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
        dpr: f64,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions first
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create scene WITHOUT background fill
        let mut scene = Scene::new();

        // Get page dimensions from layout to calculate fit-to-width scale
        let (page_width_pt, content_x, content_y) = self
            .layout_result
            .as_ref()
            .map(|layout| {
                let bounds = layout.scene.compute_bounds();
                let page_width = layout.total_width;
                (page_width, bounds.x0.max(0.0), bounds.y0.max(0.0))
            })
            .unwrap_or((595.0, 0.0, 0.0));

        // Calculate scale to fit page width to canvas width
        let canvas_css_width = canvas_width as f64 / dpr;
        let page_css_width = page_width_pt * DPI_SCALE;
        let fit_scale = canvas_css_width / page_css_width;

        // Combined scale
        let total_scale = fit_scale * DPI_SCALE * dpr;

        // Build transform
        let transform = Affine::translate((-content_x * total_scale, -content_y * total_scale))
            * Affine::scale(total_scale);

        // Render layout if available
        if let Some(ref layout) = self.layout_result {
            let base_renderer = SceneRenderBuilder::new().spatium(5.0).build();
            let mut renderer = self.font_bundle.configure_renderer(base_renderer);

            let viewport_rect = Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64);
            renderer.set_viewport(viewport_rect);

            renderer.render_with_transform(
                &mut VelloScenePainter::new(&mut scene),
                &layout.scene,
                transform,
            );
        }

        // Render with WHITE base color to prevent flashing
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render_with_base_color(&scene, Color::WHITE)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Render to a canvas element with fixed scale (no fit-to-width) (WASM only).
    ///
    /// Unlike `render_to_canvas_white`, this method does NOT scale the chart to
    /// fill the container width. It renders at 1:1 scale (with DPI adjustment).
    /// Useful when the container has CSS transforms (like skew) that distort
    /// bounding rect measurements.
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to render to
    /// * `dpr` - Device pixel ratio for high-DPI displays
    #[cfg(target_arch = "wasm32")]
    pub async fn render_to_canvas_fixed_scale(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
        dpr: f64,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions first
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create scene WITHOUT background fill
        let mut scene = Scene::new();

        // Get content offset from layout (to position at origin)
        let (content_x, content_y) = self
            .layout_result
            .as_ref()
            .map(|layout| {
                let bounds = layout.scene.compute_bounds();
                (bounds.x0.max(0.0), bounds.y0.max(0.0))
            })
            .unwrap_or((0.0, 0.0));

        // Fixed scale: just DPI_SCALE * dpr (no fit-to-width scaling)
        let total_scale = DPI_SCALE * dpr;

        // Build transform: offset content to origin, then scale
        let transform = Affine::translate((-content_x * total_scale, -content_y * total_scale))
            * Affine::scale(total_scale);

        // Render layout if available
        if let Some(ref layout) = self.layout_result {
            let base_renderer = SceneRenderBuilder::new().spatium(5.0).build();
            let mut renderer = self.font_bundle.configure_renderer(base_renderer);

            let viewport_rect = Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64);
            renderer.set_viewport(viewport_rect);

            renderer.render_with_transform(
                &mut VelloScenePainter::new(&mut scene),
                &layout.scene,
                transform,
            );
        }

        // Render with WHITE base color
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render_with_base_color(&scene, Color::WHITE)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Render a blank white page to the canvas (WASM only).
    ///
    /// Used when the chart source is invalid/incomplete (e.g., during typewriter animation).
    /// This provides a smooth visual experience instead of showing errors or stale content.
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to render to
    /// * `dpr` - Device pixel ratio for high-DPI displays
    #[cfg(target_arch = "wasm32")]
    pub async fn render_blank_white(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
        dpr: f64,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create empty scene - just renders the base color
        let scene = Scene::new();

        // Render with WHITE base color for a blank page
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render_with_base_color(&scene, Color::WHITE)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Render a blank A4 page with transform (for editor view with gray workspace).
    ///
    /// Used when the chart source is empty or invalid. Shows a white A4 page
    /// at the specified position against the gray workspace background.
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to render to
    /// * `translate_x` - X translation in pixels (already scaled by DPR)
    /// * `translate_y` - Y translation in pixels (already scaled by DPR)
    /// * `scale` - Scale factor (already includes DPR)
    /// * `is_snippet` - If true, use snippet dimensions; if false, use A4
    #[cfg(target_arch = "wasm32")]
    pub async fn render_blank_page_with_transform(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
        translate_x: f64,
        translate_y: f64,
        scale: f64,
        is_snippet: bool,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // A4 dimensions in points
        const A4_WIDTH: f64 = 595.0;
        const A4_HEIGHT: f64 = 842.0;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create scene with gray background and white page
        let mut scene = Scene::new();

        // Build transform
        let transform = Affine::translate((translate_x, translate_y)) * Affine::scale(scale);

        // Fill gray workspace background
        scene.fill(
            vello::peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(55, 65, 81),
            None,
            &Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64),
        );

        // Draw white page rectangle
        let (page_width, page_height) = if is_snippet {
            // For snippet mode, use a reasonable default size
            (400.0, 300.0)
        } else {
            // A4 page
            (A4_WIDTH, A4_HEIGHT)
        };

        scene.fill(
            vello::peniko::Fill::NonZero,
            transform,
            Color::WHITE,
            None,
            &Rect::new(0.0, 0.0, page_width, page_height),
        );

        // Render to canvas
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render(&scene)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Render to a canvas element with white background for PDF export (WASM only).
    ///
    /// Similar to `render_to_canvas_transparent` but uses white background,
    /// which is appropriate for PDF export where the canvas content becomes
    /// the final document background.
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to render to
    /// * `translate_x` - X translation in pixels (already scaled)
    /// * `translate_y` - Y translation in pixels (already scaled)
    /// * `scale` - Scale factor for high-DPI export
    #[cfg(target_arch = "wasm32")]
    pub async fn render_to_canvas_for_export(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
        translate_x: f64,
        translate_y: f64,
        scale: f64,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions first
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed (access renderer mutably)
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create scene WITHOUT background fill (Vello will use base_color)
        let mut scene = Scene::new();

        // Build transform: translate then scale
        let transform = Affine::translate((translate_x, translate_y)) * Affine::scale(scale);

        // Render layout if available
        if let Some(ref layout) = self.layout_result {
            // Create scene renderer with fonts
            let base_renderer = SceneRenderBuilder::new().spatium(5.0).build();
            let mut renderer = self.font_bundle.configure_renderer(base_renderer);

            // Set viewport for culling
            let viewport_rect = Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64);
            renderer.set_viewport(viewport_rect);

            // Render with user's view transform
            renderer.render_with_transform(
                &mut VelloScenePainter::new(&mut scene),
                &layout.scene,
                transform,
            );
        }

        // Render to canvas with TRANSPARENT base color - scene already has white page backgrounds
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render_with_base_color(&scene, Color::TRANSPARENT)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Export the current layout to SVG.
    ///
    /// Returns the SVG string or an error if no layout is available.
    pub fn export_to_svg(&self) -> Result<String, String> {
        use keyflow::engraver::export::{SvgExportConfig, SvgSerializer};

        let layout = self
            .layout_result
            .as_ref()
            .ok_or_else(|| "No layout available for export".to_string())?;

        // Get dimensions from the scene's computed bounds or use A4 defaults
        let bounds = layout.scene.compute_bounds();
        let (width, height) = if bounds.width() > 0.0 && bounds.height() > 0.0 {
            // Add some padding
            (bounds.x1 + 20.0, bounds.y1 + 20.0)
        } else {
            // A4 dimensions in points
            (595.0, 842.0)
        };

        let config = SvgExportConfig {
            width,
            height,
            view_box: None, // Default viewBox
            include_semantic_ids: true,
            embed_glyphs: false,
            precision: 2,
            pretty_print: true,
            background: Some(vello::peniko::Color::WHITE),
            default_stroke_width: 0.5,
            embedded_fonts: vec![
                // SMuFL music symbol font
                (
                    "Bravura".to_string(),
                    self.font_bundle.symbol_font_data().as_ref().clone(),
                ),
                // Chord symbol fonts
                (
                    "MuseJazzText".to_string(),
                    self.font_bundle.text_font_data().as_ref().clone(),
                ),
                (
                    "MuseJazz".to_string(),
                    self.font_bundle.text_font_data().as_ref().clone(),
                ),
                // Text fonts - FreeSans with all aliases used in the chart
                (
                    "FreeSans".to_string(),
                    self.font_bundle.aux_font_data().as_ref().clone(),
                ),
                (
                    "title-bold".to_string(),
                    self.font_bundle.aux_font_data().as_ref().clone(),
                ),
                (
                    "part-name-bold".to_string(),
                    self.font_bundle.aux_font_data().as_ref().clone(),
                ),
                (
                    "sans-serif".to_string(),
                    self.font_bundle.aux_font_data().as_ref().clone(),
                ),
            ],
        };

        let mut serializer = SvgSerializer::new(config);
        Ok(serializer.serialize(&layout.scene))
    }

    /// Export each page as a separate SVG string (LilyPond-style).
    ///
    /// Returns a vector of SVG strings, one per page. Each SVG has:
    /// - Proper dimensions matching the page size
    /// - viewBox set to show only that page's content
    /// - Semantic IDs on elements for editability
    ///
    /// This is the recommended export format for high-quality vector output
    /// that can be edited in vector graphics software or converted to PDF.
    pub fn export_pages_to_svg(&self) -> Result<Vec<String>, String> {
        use keyflow::engraver::export::{SvgExportConfig, SvgSerializer};

        let layout = self
            .layout_result
            .as_ref()
            .ok_or_else(|| "No layout available for export".to_string())?;

        let page_info = self.get_page_info();

        if page_info.is_empty() {
            // Single page / snippet mode - export the whole scene
            return self.export_to_svg().map(|svg| vec![svg]);
        }

        let mut svg_pages = Vec::with_capacity(page_info.len());

        for (_page_num, page_x, page_y, page_width, page_height) in page_info {
            // Create config for this page with viewBox set to clip to this page's area
            // Include embedded fonts so SVGs are self-contained
            let config = SvgExportConfig::for_page(page_x, page_y, page_width, page_height)
                // SMuFL music symbol font
                .with_embedded_font(
                    "Bravura",
                    self.font_bundle.symbol_font_data().as_ref().clone(),
                )
                // Chord symbol fonts
                .with_embedded_font(
                    "MuseJazzText",
                    self.font_bundle.text_font_data().as_ref().clone(),
                )
                .with_embedded_font(
                    "MuseJazz",
                    self.font_bundle.text_font_data().as_ref().clone(),
                )
                // Text fonts - FreeSans with all aliases used in the chart
                .with_embedded_font(
                    "FreeSans",
                    self.font_bundle.aux_font_data().as_ref().clone(),
                )
                .with_embedded_font(
                    "title-bold",
                    self.font_bundle.aux_font_data().as_ref().clone(),
                )
                .with_embedded_font(
                    "part-name-bold",
                    self.font_bundle.aux_font_data().as_ref().clone(),
                )
                .with_embedded_font(
                    "sans-serif",
                    self.font_bundle.aux_font_data().as_ref().clone(),
                );

            let mut serializer = SvgSerializer::new(config);
            let svg = serializer.serialize(&layout.scene);
            svg_pages.push(svg);
        }

        Ok(svg_pages)
    }

    /// Export the current layout to PDF (vector-based).
    ///
    /// Returns the PDF bytes or an error if no layout is available.
    /// Note: For pixel-perfect output matching the screen render, use `export_to_pdf_rasterized` instead.
    pub fn export_to_pdf(&self) -> Result<Vec<u8>, String> {
        use keyflow::engraver::export::{PdfExportConfig, PdfSerializer};

        let layout = self
            .layout_result
            .as_ref()
            .ok_or_else(|| "No layout available for export".to_string())?;

        // Get dimensions from the scene's computed bounds or use A4 defaults
        let bounds = layout.scene.compute_bounds();
        let (width, height) = if bounds.width() > 0.0 && bounds.height() > 0.0 {
            // Add some padding
            (bounds.x1 + 20.0, bounds.y1 + 20.0)
        } else {
            // A4 dimensions in points
            (595.0, 842.0)
        };

        // Configure PDF export with embedded fonts
        let config = PdfExportConfig {
            width,
            height,
            title: "Chart Export".to_string(),
            author: Some("FastTrackStudio".to_string()),
            background: Some(vello::peniko::Color::WHITE),
            default_stroke_width: 0.5,
            // Use MuseJazz as the default text font
            font_data: Some(self.font_bundle.text_font_data().clone()),
            // Use Bravura for SMuFL music symbols
            smufl_font_data: Some(self.font_bundle.symbol_font_data().clone()),
        };

        let mut serializer = PdfSerializer::new(config);

        // Add additional named fonts for font-family matching
        if let Err(e) = serializer.add_named_font("MuseJazzText", self.font_bundle.text_font_data())
        {
            tracing::warn!("Failed to add MuseJazzText font: {}", e);
        }
        if let Err(e) = serializer.add_named_font("MuseJazz", self.font_bundle.text_font_data()) {
            tracing::warn!("Failed to add MuseJazz font: {}", e);
        }
        if let Err(e) = serializer.add_named_font("FreeSans", self.font_bundle.aux_font_data()) {
            tracing::warn!("Failed to add FreeSans font: {}", e);
        }

        serializer
            .serialize(&layout.scene)
            .map_err(|e| format!("PDF export failed: {e}"))
    }

    /// Export the current layout to PDF using a rasterized image.
    ///
    /// This method creates a PDF by embedding a PNG image rendered from the canvas,
    /// guaranteeing pixel-perfect output that matches the screen render exactly.
    ///
    /// # Arguments
    /// * `png_bytes` - PNG-encoded image data from the canvas
    /// * `dpi` - The DPI at which the image was rendered
    /// * `page_width` - PDF page width in points
    /// * `page_height` - PDF page height in points
    ///
    /// # Returns
    /// PDF bytes ready to be downloaded.
    pub fn export_to_pdf_rasterized(
        &self,
        png_bytes: &[u8],
        dpi: f64,
        page_width: f64,
        page_height: f64,
    ) -> Result<Vec<u8>, String> {
        use keyflow::engraver::export::{PdfExportConfig, PdfSerializer};

        // Configure PDF export with specified page dimensions
        let config = PdfExportConfig {
            width: page_width,
            height: page_height,
            title: "Chart Export".to_string(),
            author: Some("FastTrackStudio".to_string()),
            background: None, // Image already has background
            default_stroke_width: 0.5,
            font_data: None,
            smufl_font_data: None,
        };

        let serializer = PdfSerializer::new(config);

        serializer
            .serialize_from_png(png_bytes, dpi)
            .map_err(|e| format!("PDF export failed: {e}"))
    }

    /// Get the content dimensions in points for the current layout.
    ///
    /// Returns (width, height) in points, useful for determining render size.
    /// Includes 20pt padding for comfortable viewing.
    pub fn get_content_dimensions(&self) -> Option<(f64, f64)> {
        self.layout_result.as_ref().map(|layout| {
            let bounds = layout.scene.compute_bounds();
            if bounds.width() > 0.0 && bounds.height() > 0.0 {
                (bounds.x1 + 20.0, bounds.y1 + 20.0)
            } else {
                (595.0, 842.0)
            }
        })
    }

    /// Get the tight content dimensions in points (no padding).
    ///
    /// Returns (width, height) of just the content bounds, useful for PDF export.
    pub fn get_content_dimensions_tight(&self) -> Option<(f64, f64)> {
        self.layout_result.as_ref().map(|layout| {
            let bounds = layout.scene.compute_bounds();
            if bounds.width() > 0.0 && bounds.height() > 0.0 {
                (bounds.width(), bounds.height())
            } else {
                (595.0, 842.0)
            }
        })
    }

    /// Get the content origin offset in points for the current layout.
    ///
    /// Returns (x, y) offset to the top-left of the actual content.
    /// Use negative of these values to translate content to canvas origin.
    pub fn get_content_origin(&self) -> Option<(f64, f64)> {
        self.layout_result.as_ref().map(|layout| {
            let bounds = layout.scene.compute_bounds();
            (bounds.x0, bounds.y0)
        })
    }

    /// Get page information for multi-page PDF export.
    ///
    /// Returns a vector of (page_number, x_offset, y_offset, width, height) for each page.
    /// Uses the offsets stored in each PageLayout, which were calculated during layout
    /// using the correct page gap and offset values.
    pub fn get_page_info(&self) -> Vec<(u32, f64, f64, f64, f64)> {
        self.layout_result
            .as_ref()
            .map(|layout| {
                layout
                    .pages
                    .iter()
                    .map(|page| {
                        (
                            page.number,
                            page.x_offset,
                            page.y_offset,
                            page.width,
                            page.height,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Render a specific page to a canvas for PDF export (WASM only).
    ///
    /// # Arguments
    /// * `canvas` - The HTML canvas element to render to
    /// * `page_x_offset` - X offset of the page in the scene (in points)
    /// * `page_y_offset` - Y offset of the page in the scene (in points)
    /// * `scale` - Scale factor for high-DPI export
    #[cfg(target_arch = "wasm32")]
    pub async fn render_page_to_canvas(
        &mut self,
        canvas: &web_sys::HtmlCanvasElement,
        page_x_offset: f64,
        page_y_offset: f64,
        scale: f64,
    ) -> Result<(), String> {
        use wasm::WebGpuRenderer;

        // Initialize renderer if needed
        if self.wgpu_renderer.is_none() {
            let renderer = WebGpuRenderer::new(canvas.clone()).await?;
            self.wgpu_renderer = Some(renderer);
        }

        // Get canvas dimensions
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();

        // Resize if needed
        if let Some(renderer) = self.wgpu_renderer.as_mut() {
            let (current_width, current_height) = renderer.dimensions();
            if current_width != canvas_width || current_height != canvas_height {
                renderer.resize(canvas_width, canvas_height);
            }
        }

        // Create scene
        let mut scene = Scene::new();

        // Transform: offset to bring this page to origin, then scale
        let transform = Affine::translate((-page_x_offset * scale, -page_y_offset * scale))
            * Affine::scale(scale);

        // Render layout if available
        if let Some(ref layout) = self.layout_result {
            let base_renderer = SceneRenderBuilder::new().spatium(5.0).build();
            let mut renderer = self.font_bundle.configure_renderer(base_renderer);

            // Set viewport for culling
            let viewport_rect = Rect::new(0.0, 0.0, canvas_width as f64, canvas_height as f64);
            renderer.set_viewport(viewport_rect);

            renderer.render_with_transform(
                &mut VelloScenePainter::new(&mut scene),
                &layout.scene,
                transform,
            );
        }

        // Render with transparent base (scene has white page backgrounds)
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            wgpu_renderer.render_with_base_color(&scene, Color::TRANSPARENT)
        } else {
            Err("WebGPU renderer not initialized".to_string())
        }
    }

    /// Create a multi-page PDF from rendered PNG images.
    ///
    /// # Arguments
    /// * `pages` - Vector of (png_bytes, width, height) for each page
    ///
    /// # Returns
    /// PDF bytes ready to be downloaded.
    pub fn export_multi_page_pdf(
        &self,
        pages: Vec<(Vec<u8>, f64, f64)>,
    ) -> Result<Vec<u8>, String> {
        use keyflow::engraver::export::PdfSerializer;

        PdfSerializer::serialize_multi_page_from_png(&pages)
            .map_err(|e| format!("PDF export failed: {e}"))
    }

    /// Export to PDF using SVG as an intermediate format (vector-based).
    ///
    /// This approach converts the scene graph to SVG, then uses svg2pdf to
    /// convert the SVG to PDF. This preserves vector quality and ensures
    /// fonts are properly embedded.
    ///
    /// # Returns
    /// PDF bytes ready to be downloaded.
    pub fn export_to_pdf_via_svg(&self) -> Result<Vec<u8>, String> {
        use keyflow::engraver::export::{PdfSerializer, SvgExportConfig, SvgSerializer};

        let layout = self
            .layout_result
            .as_ref()
            .ok_or_else(|| "No layout available for export".to_string())?;

        // Generate SVG for each page
        let mut svg_pages = Vec::new();
        let page_info = self.get_page_info();

        if page_info.is_empty() {
            // Single page - export entire scene
            let bounds = layout.scene.compute_bounds();
            let (width, height) = if bounds.width() > 0.0 && bounds.height() > 0.0 {
                (bounds.width(), bounds.height())
            } else {
                (595.0, 842.0)
            };

            let config = SvgExportConfig {
                width,
                height,
                view_box: None,
                include_semantic_ids: false,
                embed_glyphs: false,
                precision: 2,
                pretty_print: false,
                background: Some(vello::peniko::Color::WHITE),
                default_stroke_width: 0.5,
                embedded_fonts: Vec::new(),
            };

            let mut serializer = SvgSerializer::new(config);
            let svg = serializer.serialize(&layout.scene);
            svg_pages.push(svg);
        } else {
            // Multi-page - extract and export each page separately
            // For now, we export the entire scene and let svg2pdf handle pagination
            // A proper implementation would create separate scene nodes per page
            let bounds = layout.scene.compute_bounds();
            let config = SvgExportConfig {
                width: bounds.width(),
                height: bounds.height(),
                view_box: None,
                include_semantic_ids: false,
                embed_glyphs: false,
                precision: 2,
                pretty_print: false,
                background: Some(vello::peniko::Color::WHITE),
                default_stroke_width: 0.5,
                embedded_fonts: Vec::new(),
            };

            let mut serializer = SvgSerializer::new(config);
            let svg = serializer.serialize(&layout.scene);
            svg_pages.push(svg);
        }

        // Build font data for svg2pdf
        // Note: Font names here are for logging - fontdb extracts actual family names from font files
        // The important thing is that all required font data is loaded
        let fonts: Vec<(&str, &[u8])> = vec![
            ("Bravura", self.font_bundle.symbol_font_data().as_slice()),
            // MuseJazz font file has internal family name "MuseJazz Text" (with space)
            (
                "MuseJazz Text",
                self.font_bundle.text_font_data().as_slice(),
            ),
            ("FreeSans", self.font_bundle.aux_font_data().as_slice()),
        ];

        PdfSerializer::serialize_from_svg(&svg_pages, &fonts)
            .map_err(|e| format!("SVG to PDF export failed: {e}"))
    }

    /// Export to multi-page PDF using SVG as an intermediate format.
    ///
    /// For each page in the layout:
    /// 1. Extracts the page's scene content
    /// 2. Converts to SVG
    /// 3. Combines all SVGs into a multi-page PDF
    ///
    /// # Returns
    /// PDF bytes ready to be downloaded.
    pub fn export_multi_page_pdf_via_svg(&self) -> Result<Vec<u8>, String> {
        use keyflow::engraver::export::{PdfSerializer, SvgExportConfig, SvgSerializer};

        let layout = self
            .layout_result
            .as_ref()
            .ok_or_else(|| "No layout available for export".to_string())?;

        let page_info = self.get_page_info();

        if page_info.is_empty() {
            // Fall back to single-page export
            return self.export_to_pdf_via_svg();
        }

        // For multi-page export, we need to extract each page's scene content
        // Currently the scene has all pages side-by-side
        // We'll generate SVGs with viewport transforms for each page

        let mut svg_pages = Vec::new();

        for (_page_num, page_x, page_y, page_width, page_height) in &page_info {
            // Create SVG with viewBox set to this page's area
            let config = SvgExportConfig::for_page(*page_x, *page_y, *page_width, *page_height);

            // Create a translated view of the scene for this page
            // The SVG serializer will output the full scene, but we set the viewBox
            // to show only this page's content
            let svg_content = {
                let mut serializer = SvgSerializer::new(config);
                let base_svg = serializer.serialize(&layout.scene);

                // Modify the SVG's viewBox to show only this page
                // Replace the viewBox in the SVG header
                let viewbox = format!(
                    "viewBox=\"{:.2} {:.2} {:.2} {:.2}\"",
                    page_x, page_y, page_width, page_height
                );

                // Find and replace the viewBox attribute
                if let Some(start) = base_svg.find("viewBox=\"") {
                    if let Some(end) = base_svg[start..].find('"').and_then(|s| {
                        base_svg[start + s + 1..]
                            .find('"')
                            .map(|e| start + s + 1 + e + 1)
                    }) {
                        let mut modified = base_svg[..start].to_string();
                        modified.push_str(&viewbox);
                        modified.push_str(&base_svg[end..]);
                        modified
                    } else {
                        base_svg
                    }
                } else {
                    base_svg
                }
            };

            svg_pages.push(svg_content);
        }

        // Build font data for svg2pdf
        // Note: Font names here are for logging - fontdb extracts actual family names from font files
        // The important thing is that all required font data is loaded
        let fonts: Vec<(&str, &[u8])> = vec![
            ("Bravura", self.font_bundle.symbol_font_data().as_slice()),
            // MuseJazz font file has internal family name "MuseJazz Text" (with space)
            (
                "MuseJazz Text",
                self.font_bundle.text_font_data().as_slice(),
            ),
            ("FreeSans", self.font_bundle.aux_font_data().as_slice()),
        ];

        PdfSerializer::serialize_from_svg(&svg_pages, &fonts)
            .map_err(|e| format!("SVG to PDF export failed: {e}"))
    }
}

impl Default for ChartLayoutManager {
    fn default() -> Self {
        Self::new().expect("Failed to initialize chart layout manager")
    }
}

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    //! WASM-specific Vello/WebGPU rendering.

    use super::*;
    use web_sys::HtmlCanvasElement;
    use wgpu::{TextureDescriptor, TextureDimension, TextureUsages, TextureViewDescriptor};

    /// WebGPU renderer state for WASM.
    pub struct WebGpuRenderer {
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        surface_config: wgpu::SurfaceConfiguration,
        vello_renderer: vello::Renderer,
        render_texture: wgpu::Texture,
        blitter: wgpu::util::TextureBlitter,
        width: u32,
        height: u32,
    }

    impl WebGpuRenderer {
        /// Create a new WebGPU renderer from a canvas element.
        pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, String> {
            // Get canvas dimensions
            let width = canvas.width().max(1);
            let height = canvas.height().max(1);

            // Create wgpu instance
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
                ..Default::default()
            });

            // Create surface from canvas
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
                .map_err(|e| format!("Failed to create surface: {e}"))?;

            // Request adapter
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .map_err(|e| format!("Failed to find suitable GPU adapter: {e}"))?;

            // Request device
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("Docsite Chart Renderer"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                    ..Default::default()
                })
                .await
                .map_err(|e| format!("Failed to create device: {e}"))?;

            // Configure surface (prefer non-sRGB for Vello)
            let surface_caps = surface.get_capabilities(&adapter);
            let surface_format = surface_caps
                .formats
                .iter()
                .find(|f| !f.is_srgb())
                .copied()
                .unwrap_or(surface_caps.formats[0]);

            let surface_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width,
                height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &surface_config);

            // Create Vello renderer with default options
            let vello_renderer = vello::Renderer::new(&device, vello::RendererOptions::default())
                .map_err(|e| format!("Failed to create Vello renderer: {e}"))?;

            // Create intermediate render texture (Rgba8Unorm for Vello's compute shaders)
            let render_texture = Self::create_render_texture(&device, width, height);

            // Create blitter for copying from intermediate texture to surface
            let blitter = wgpu::util::TextureBlitter::new(&device, surface_format);

            Ok(Self {
                device,
                queue,
                surface,
                surface_config,
                vello_renderer,
                render_texture,
                blitter,
                width,
                height,
            })
        }

        /// Create the intermediate render texture for Vello.
        fn create_render_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
            device.create_texture(&TextureDescriptor {
                label: Some("Vello Render Texture"),
                size: wgpu::Extent3d {
                    width: width.max(1),
                    height: height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        }

        /// Resize the renderer to match new canvas dimensions.
        pub fn resize(&mut self, width: u32, height: u32) {
            if width == 0 || height == 0 {
                return;
            }
            self.width = width;
            self.height = height;
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
            self.render_texture = Self::create_render_texture(&self.device, width, height);
        }

        /// Render a Vello scene to the canvas.
        pub fn render(&mut self, scene: &Scene) -> Result<(), String> {
            self.render_with_base_color(scene, Color::from_rgb8(55, 65, 81))
        }

        /// Render a Vello scene to the canvas with a custom base color.
        pub fn render_with_base_color(
            &mut self,
            scene: &Scene,
            base_color: Color,
        ) -> Result<(), String> {
            let surface_texture = self
                .surface
                .get_current_texture()
                .map_err(|e| format!("Failed to get surface texture: {e}"))?;

            // Create views for rendering
            let render_view = self
                .render_texture
                .create_view(&TextureViewDescriptor::default());
            let surface_view = surface_texture
                .texture
                .create_view(&TextureViewDescriptor::default());

            let render_params = vello::RenderParams {
                base_color,
                width: self.width,
                height: self.height,
                antialiasing_method: vello::AaConfig::Msaa16,
            };

            // Render to intermediate texture
            self.vello_renderer
                .render_to_texture(
                    &self.device,
                    &self.queue,
                    scene,
                    &render_view,
                    &render_params,
                )
                .map_err(|e| format!("Failed to render: {e}"))?;

            // Blit to surface
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blit Encoder"),
                });
            self.blitter
                .copy(&self.device, &mut encoder, &render_view, &surface_view);
            self.queue.submit(std::iter::once(encoder.finish()));

            surface_texture.present();
            Ok(())
        }

        /// Get current dimensions.
        pub fn dimensions(&self) -> (u32, u32) {
            (self.width, self.height)
        }
    }
}
