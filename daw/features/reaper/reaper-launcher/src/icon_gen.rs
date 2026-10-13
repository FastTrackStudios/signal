//! Icon generation for REAPER instance launchers.
//!
//! Composites a color tint + badge pill with text onto the embedded REAPER base
//! icon, matching the macOS Swift icon generation output. Uses `tiny-skia` for
//! raster compositing and `ab_glyph` for text rendering with an embedded font.
//!
//! Requires the `icon-gen` feature flag.

/// Configuration for generating a badged icon.
pub struct IconConfig {
    /// Badge text, e.g., "TEST", "GUITAR".
    pub badge_text: String,
    /// Badge background color as RGB tuple.
    pub color: (u8, u8, u8),
    /// Pixel sizes to render, e.g., `[48, 128, 256]`.
    pub sizes: Vec<u32>,
    /// If true, skip the color tint on the background (plain dark gray).
    pub no_tint: bool,
}

/// Known rig appearances for the standard FTS rig types.
///
/// Returns `(color_rgb, badge_text)` for a given rig type identifier.
pub fn rig_appearance(rig_type: &str) -> Option<((u8, u8, u8), &'static str)> {
    Some(match rig_type {
        "reaper" => ((0xdd, 0xdd, 0xdd), "FTS"),
        "guitar" => ((0x3b, 0x82, 0xf6), "GUITAR"),
        "bass" => ((0xea, 0xb3, 0x08), "BASS"),
        "keys" => ((0x22, 0xc5, 0x5e), "KEYS"),
        "drums" => ((0xef, 0x44, 0x44), "DRUMS"),
        "drum-enhancement" => ((0xf9, 0x73, 0x16), "DRUM+"),
        "vocals" => ((0xec, 0x48, 0x99), "VOCALS"),
        "session" => ((0x66, 0x9e, 0xe6), "TRACKS"),
        "dev" => ((0x4d, 0x4d, 0x4d), "DEV"),
        "testing" => ((0x4d, 0x4d, 0x4d), "TEST"),
        _ => return None,
    })
}

/// Generate PNGs at the requested sizes and install them to the XDG icon theme
/// directory (`~/.local/share/icons/hicolor/{size}x{size}/apps/{id}.png`).
///
/// Also ensures `index.theme` exists for KDE Plasma compatibility.
#[cfg(feature = "icon-gen")]
pub fn generate_and_install_icons(id: &str, config: &IconConfig) -> Result<(), String> {
    let home =
        std::env::var("HOME").map_err(|_| "HOME environment variable not set".to_string())?;
    let icon_base = std::path::PathBuf::from(&home).join(".local/share/icons/hicolor");

    for &size in &config.sizes {
        let dir = icon_base.join(format!("{size}x{size}/apps"));
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;

        let png_path = dir.join(format!("{id}.png"));
        let png_data = render_icon_png(size, config)
            .map_err(|e| format!("Failed to render {size}x{size} icon: {e}"))?;
        std::fs::write(&png_path, &png_data)
            .map_err(|e| format!("Failed to write {}: {e}", png_path.display()))?;
    }

    // Ensure index.theme exists (required for KDE Plasma)
    ensure_index_theme(&icon_base)?;

    // Refresh icon cache if gtk-update-icon-cache is available
    let _ = std::process::Command::new("gtk-update-icon-cache")
        .args(["-f", "-t"])
        .arg(&icon_base)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    Ok(())
}

#[cfg(feature = "icon-gen")]
fn ensure_index_theme(icon_base: &std::path::Path) -> Result<(), String> {
    let path = icon_base.join("index.theme");
    if path.exists() {
        return Ok(());
    }
    std::fs::write(
        &path,
        "[Icon Theme]\n\
         Name=Hicolor\n\
         Comment=Fallback icon theme\n\
         Hidden=true\n\
         Directories=48x48/apps,128x128/apps,256x256/apps\n\
         \n\
         [48x48/apps]\n\
         Size=48\n\
         Context=Apps\n\
         Type=Threshold\n\
         \n\
         [128x128/apps]\n\
         Size=128\n\
         Context=Apps\n\
         Type=Threshold\n\
         \n\
         [256x256/apps]\n\
         Size=256\n\
         Context=Apps\n\
         Type=Threshold\n",
    )
    .map_err(|e| format!("Failed to write index.theme: {e}"))
}

// ── Pure-Rust icon rendering ────────────────────────────────────────────────

#[cfg(feature = "icon-gen")]
const BASE_ICNS: &[u8] = include_bytes!("../assets/reaper-base.icns");

#[cfg(feature = "icon-gen")]
const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");

#[cfg(feature = "icon-gen")]
fn render_icon_png(size: u32, config: &IconConfig) -> Result<Vec<u8>, String> {
    use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
    use std::io::Cursor;
    use tiny_skia::*;

    let sz = size as f32;
    let (cr, cg, cb) = config.color;
    let r = cr as f32 / 255.0;
    let g = cg as f32 / 255.0;
    let b = cb as f32 / 255.0;

    // ── MacTahoe-style rounded square background ──
    // Match MacTahoe proportions: 56/64 = 87.5% icon area, ~20% corner radius
    let margin = sz * 0.0625; // 4px at 64px
    let bg_size = sz - margin * 2.0;
    let corner_r = bg_size * 0.20;

    let mut pixmap =
        Pixmap::new(size, size).ok_or_else(|| "Failed to create pixmap".to_string())?;

    let bg_rect = rounded_rect(margin, margin, bg_size, bg_size, corner_r);
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };

    // Background: dark gray with a subtle color tint (skip tint if no_tint flag set)
    let base = 0.165_f32;
    let tint_strength = if config.no_tint { 0.0 } else { 0.08 };
    paint.set_color(
        Color::from_rgba(
            base + r * tint_strength,
            base + g * tint_strength,
            base + b * tint_strength,
            1.0,
        )
        .ok_or_else(|| "Invalid bg color".to_string())?,
    );
    pixmap.fill_path(
        &bg_rect,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    // ── Inset REAPER icon ──
    // Scale REAPER icon to ~65% of canvas, centered
    let icon_scale = 0.65;
    let icon_px = (sz * icon_scale) as u32;
    let reaper_icon = load_base_icon(icon_px)?;
    let offset = ((sz - icon_px as f32) / 2.0) as i32;
    // Offset icon slightly upward to leave room for badge
    let y_nudge = (sz * 0.04) as i32;

    // Clip to rounded rect: draw reaper icon only inside the background shape
    pixmap.draw_pixmap(
        offset,
        offset - y_nudge,
        reaper_icon.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        // Clip mask: only draw inside the rounded square
        Some(&clip_mask_from_path(size, &bg_rect)?),
    );

    // Subtle border on the rounded square
    let border_color =
        Color::from_rgba(1.0, 1.0, 1.0, 0.08).ok_or_else(|| "Invalid border color".to_string())?;
    paint.set_color(border_color);
    let stroke = Stroke {
        width: (sz * 0.015).max(1.0),
        ..Default::default()
    };
    pixmap.stroke_path(&bg_rect, &paint, &stroke, Transform::identity(), None);

    // ── Badge pill ──
    let lines: Vec<&str> = config.badge_text.lines().collect();
    let line_count = lines.len();
    let is_multiline = line_count > 1;

    let badge_w = bg_size * 0.90;
    let badge_h = if is_multiline {
        bg_size * 0.24 * line_count as f32
    } else {
        bg_size * 0.34
    };
    let badge_x = (sz - badge_w) / 2.0;
    let badge_y = margin + bg_size - badge_h - bg_size * 0.04;
    let badge_radius = if is_multiline {
        badge_h * 0.25
    } else {
        badge_h / 2.0
    };

    let badge_rect = rounded_rect(badge_x, badge_y, badge_w, badge_h, badge_radius);

    // Badge shadow
    let mut shadow_paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    for offset in [2.0_f32, 1.0] {
        shadow_paint.set_color(
            Color::from_rgba(0.0, 0.0, 0.0, 0.2)
                .ok_or_else(|| "Invalid shadow color".to_string())?,
        );
        let sr = rounded_rect(badge_x, badge_y + offset, badge_w, badge_h, badge_radius);
        pixmap.fill_path(
            &sr,
            &shadow_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // Badge fill
    paint.set_color(
        Color::from_rgba(r, g, b, 0.95).ok_or_else(|| "Invalid badge color".to_string())?,
    );
    pixmap.fill_path(
        &badge_rect,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    // Badge border (darker shade)
    paint.set_color(
        Color::from_rgba(
            (r * 0.5).min(1.0),
            (g * 0.5).min(1.0),
            (b * 0.5).min(1.0),
            0.8,
        )
        .ok_or_else(|| "Invalid badge border color".to_string())?,
    );
    let badge_stroke = Stroke {
        width: (sz * 0.012).max(1.0),
        ..Default::default()
    };
    pixmap.stroke_path(
        &badge_rect,
        &paint,
        &badge_stroke,
        Transform::identity(),
        None,
    );

    // ── Badge text ──
    let font =
        FontRef::try_from_slice(INTER_BOLD).map_err(|e| format!("Failed to load font: {e}"))?;
    let font_size = if is_multiline {
        badge_h * 0.38
    } else {
        badge_h * 0.65
    };
    let scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(scale);

    let line_height = scaled_font.height();
    let total_text_height = line_height * line_count as f32;
    let text_start_y = badge_y + (badge_h - total_text_height) / 2.0 + scaled_font.ascent();

    for (i, line) in lines.iter().enumerate() {
        let y = text_start_y + i as f32 * line_height;
        draw_text_centered(&mut pixmap, &scaled_font, line, sz / 2.0, y);
    }

    // ── Encode to PNG ──
    let img = image::RgbaImage::from_raw(size, size, pixmap.data().to_vec())
        .ok_or_else(|| "Failed to create image from pixmap".to_string())?;

    let mut png_bytes = Vec::new();
    let cursor = Cursor::new(&mut png_bytes);
    let encoder = image::codecs::png::PngEncoder::new(cursor);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        size,
        size,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| format!("Failed to encode PNG: {e}"))?;

    Ok(png_bytes)
}

/// Create a clip mask from a path (used to clip the REAPER icon to the rounded square).
#[cfg(feature = "icon-gen")]
fn clip_mask_from_path(size: u32, path: &tiny_skia::Path) -> Result<tiny_skia::Mask, String> {
    use tiny_skia::*;

    let mut tmp = Pixmap::new(size, size)
        .ok_or_else(|| "Failed to create temp pixmap for mask".to_string())?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    tmp.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);

    Ok(Mask::from_pixmap(tmp.as_ref(), MaskType::Alpha))
}

/// Load the largest image from the embedded REAPER .icns and scale to `size`.
///
/// Tries all known icon types from largest to smallest, handling both
/// uncompressed RGBA and PNG-compressed formats (ic08, ic09, etc.).
#[cfg(feature = "icon-gen")]
fn load_base_icon(size: u32) -> Result<tiny_skia::Pixmap, String> {
    use std::io::Cursor;
    use tiny_skia::*;

    let family = icns::IconFamily::read(Cursor::new(BASE_ICNS))
        .map_err(|e| format!("Failed to read .icns: {e}"))?;

    // Try all types from largest to smallest
    let icon_types = [
        icns::IconType::RGBA32_512x512_2x,
        icns::IconType::RGBA32_512x512,
        icns::IconType::RGBA32_256x256_2x,
        icns::IconType::RGBA32_256x256,
        icns::IconType::RGBA32_128x128,
    ];

    let mut best_image = None;
    for ty in &icon_types {
        if let Ok(img) = family.get_icon_with_type(*ty) {
            best_image = Some(img);
            break;
        }
    }

    // If structured types failed, try available_icons() to find any decodable icon
    if best_image.is_none() {
        let available = family.available_icons();
        // Sort by size descending to get the largest
        let mut types: Vec<_> = available.into_iter().collect();
        types.sort_by(|a, b| {
            let sa = a.pixel_width();
            let sb = b.pixel_width();
            sb.cmp(&sa)
        });
        for ty in types {
            if let Ok(img) = family.get_icon_with_type(ty) {
                best_image = Some(img);
                break;
            }
        }
    }

    let img = best_image.ok_or_else(|| "No suitable icon found in .icns".to_string())?;
    let w = img.width();
    let h = img.height();

    let rgba = image::RgbaImage::from_raw(w, h, img.data().to_vec())
        .ok_or_else(|| "Failed to create image from icns data".to_string())?;

    let resized = image::imageops::resize(&rgba, size, size, image::imageops::FilterType::Lanczos3);

    let mut pixmap =
        Pixmap::new(size, size).ok_or_else(|| "Failed to create pixmap".to_string())?;

    for (i, pixel) in resized.pixels().enumerate() {
        let [r, g, b, a] = pixel.0;
        let color = ColorU8::from_rgba(r, g, b, a).premultiply();
        pixmap.pixels_mut()[i] = color;
    }

    Ok(pixmap)
}

/// Draw white text centered horizontally at (cx, baseline_y).
#[cfg(feature = "icon-gen")]
fn draw_text_centered(
    pixmap: &mut tiny_skia::Pixmap,
    font: &ab_glyph::PxScaleFont<&ab_glyph::FontRef>,
    text: &str,
    cx: f32,
    baseline_y: f32,
) {
    use ab_glyph::ScaleFont;

    let mut total_width = 0.0_f32;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for ch in text.chars() {
        let glyph_id = font.glyph_id(ch);
        if let Some(p) = prev {
            total_width += font.kern(p, glyph_id);
        }
        total_width += font.h_advance(glyph_id);
        prev = Some(glyph_id);
    }

    let start_x = cx - total_width / 2.0;
    let w = pixmap.width();
    let h = pixmap.height();
    let pixels = pixmap.pixels_mut();

    let mut x = start_x;
    prev = None;
    for ch in text.chars() {
        let glyph_id = font.glyph_id(ch);
        if let Some(p) = prev {
            x += font.kern(p, glyph_id);
        }

        let glyph = glyph_id.with_scale_and_position(font.scale(), ab_glyph::point(x, baseline_y));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                    let alpha = (coverage * 255.0) as u8;
                    if alpha > 0 {
                        let idx = (py as u32 * w + px as u32) as usize;
                        let existing = pixels[idx];
                        let sa = alpha as f32 / 255.0;
                        let da = existing.alpha() as f32 / 255.0;
                        let out_a = sa + da * (1.0 - sa);
                        if out_a > 0.0 {
                            let blend = |src: u8, dst: u8| -> u8 {
                                ((src as f32 * sa + dst as f32 * da * (1.0 - sa)) / out_a) as u8
                            };
                            let nr = blend(255, existing.red());
                            let ng = blend(255, existing.green());
                            let nb = blend(255, existing.blue());
                            let na = (out_a * 255.0) as u8;
                            pixels[idx] =
                                tiny_skia::ColorU8::from_rgba(nr, ng, nb, na).premultiply();
                        }
                    }
                }
            });
        }

        x += font.h_advance(glyph_id);
        prev = Some(glyph_id);
    }
}

/// Create a rounded rectangle path.
#[cfg(feature = "icon-gen")]
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}
