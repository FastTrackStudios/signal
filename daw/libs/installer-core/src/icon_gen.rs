//! Runtime icon generation for rig wrapper .app bundles.
//!
//! Loads the REAPER base icon from the installed REAPER.app, applies a colored
//! tint + badge, and packs into .icns format.

use std::io::Cursor;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tiny_skia::{
    BlendMode, Color, ColorU8, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};

/// RGB + badge for a rig type.
struct RigStyle {
    r: f32,
    g: f32,
    b: f32,
    badge: &'static str,
}

fn rig_style(rig_type: &str) -> Option<RigStyle> {
    Some(match rig_type {
        "reaper" => RigStyle {
            r: 0.545,
            g: 0.361,
            b: 0.965,
            badge: "FTS",
        },
        "live" => RigStyle {
            r: 0.15,
            g: 0.65,
            b: 0.45,
            badge: "LIVE",
        },
        "guitar" => RigStyle {
            r: 0.231,
            g: 0.510,
            b: 0.965,
            badge: "GUITAR",
        },
        "bass" => RigStyle {
            r: 0.918,
            g: 0.702,
            b: 0.031,
            badge: "BASS",
        },
        "keys" => RigStyle {
            r: 0.133,
            g: 0.773,
            b: 0.369,
            badge: "KEYS",
        },
        "drums" => RigStyle {
            r: 0.937,
            g: 0.267,
            b: 0.267,
            badge: "DRUMS",
        },
        "drum-enhancement" => RigStyle {
            r: 0.976,
            g: 0.451,
            b: 0.086,
            badge: "DRUM\nENHANCE",
        },
        "vocals" => RigStyle {
            r: 0.925,
            g: 0.282,
            b: 0.600,
            badge: "VOCALS",
        },
        "session" => RigStyle {
            r: 0.400,
            g: 0.620,
            b: 0.900,
            badge: "TRACKS",
        },
        _ => return None,
    })
}

/// Generate a tinted+badged .icns icon for a rig type.
///
/// Reads the base icon from `base_icns_path` (the installed REAPER icon),
/// composites the rig's color tint and badge, and writes the result to `output_path`.
pub fn generate_rig_icns(
    base_icns_path: &std::path::Path,
    output_path: &std::path::Path,
    rig_type: &str,
) -> eyre::Result<()> {
    let style = rig_style(rig_type).ok_or_else(|| eyre::eyre!("Unknown rig type: {rig_type}"))?;

    let base_data = std::fs::read(base_icns_path)?;
    let family = icns::IconFamily::read(Cursor::new(&base_data))?;

    // Generate icons at standard macOS sizes
    let sizes = [
        (icns::IconType::RGBA32_16x16, 16),
        (icns::IconType::RGBA32_32x32, 32),
        (icns::IconType::RGBA32_32x32_2x, 64),
        (icns::IconType::RGBA32_128x128, 128),
        (icns::IconType::RGBA32_256x256, 256),
        (icns::IconType::RGBA32_256x256_2x, 512),
        (icns::IconType::RGBA32_512x512, 512),
        (icns::IconType::RGBA32_512x512_2x, 1024),
    ];

    // Load the largest base icon for scaling
    let base_image = load_best_base_image(&family)?;

    let font_data: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");
    let font = FontRef::try_from_slice(font_data)?;

    let mut out_family = icns::IconFamily::new();

    for (icon_type, px) in &sizes {
        let rendered = render_icon(&base_image, &style, &font, *px)?;
        let icns_image = icns::Image::from_data(icns::PixelFormat::RGBA, *px, *px, rendered)?;
        out_family.add_icon_with_type(&icns_image, *icon_type)?;
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(output_path)?;
    out_family.write(file)?;

    Ok(())
}

/// Load the largest available image from an icns family.
fn load_best_base_image(family: &icns::IconFamily) -> eyre::Result<image::RgbaImage> {
    let icon_types = [
        icns::IconType::RGBA32_512x512_2x,
        icns::IconType::RGBA32_512x512,
        icns::IconType::RGBA32_256x256_2x,
        icns::IconType::RGBA32_256x256,
        icns::IconType::RGBA32_128x128,
    ];

    for ty in &icon_types {
        if let Ok(img) = family.get_icon_with_type(*ty) {
            let rgba = image::RgbaImage::from_raw(img.width(), img.height(), img.data().to_vec())
                .ok_or_else(|| eyre::eyre!("Failed to create image from icns"))?;
            return Ok(rgba);
        }
    }

    eyre::bail!("No suitable icon found in .icns")
}

/// Render a single icon at the given pixel size.
fn render_icon(
    base: &image::RgbaImage,
    style: &RigStyle,
    font: &FontRef,
    size: u32,
) -> eyre::Result<Vec<u8>> {
    let sz = size as f32;

    // Resize base to target size
    let resized = image::imageops::resize(base, size, size, image::imageops::FilterType::Lanczos3);

    // Convert to tiny-skia pixmap
    let mut pixmap =
        Pixmap::new(size, size).ok_or_else(|| eyre::eyre!("Failed to create pixmap"))?;
    for (i, pixel) in resized.pixels().enumerate() {
        let [r, g, b, a] = pixel.0;
        pixmap.pixels_mut()[i] = ColorU8::from_rgba(r, g, b, a).premultiply();
    }

    // Apply color tint (SourceAtop)
    let tint = Color::from_rgba(style.r, style.g, style.b, 0.3).unwrap();
    let mut tint_pm = Pixmap::new(size, size).unwrap();
    tint_pm.fill(tint);
    pixmap.draw_pixmap(
        0,
        0,
        tint_pm.as_ref(),
        &PixmapPaint {
            blend_mode: BlendMode::SourceAtop,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );

    // Skip badge on very small icons
    if size >= 64 {
        draw_badge(&mut pixmap, style, font, sz);
    }

    // Convert back to unpremultiplied RGBA
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for (i, px) in pixmap.pixels().iter().enumerate() {
        let c = px.demultiply();
        rgba[i * 4] = c.red();
        rgba[i * 4 + 1] = c.green();
        rgba[i * 4 + 2] = c.blue();
        rgba[i * 4 + 3] = c.alpha();
    }

    Ok(rgba)
}

fn draw_badge(pixmap: &mut Pixmap, style: &RigStyle, font: &FontRef, sz: f32) {
    let lines: Vec<&str> = style.badge.lines().collect();
    let line_count = lines.len();
    let is_multiline = line_count > 1;

    let badge_w = sz * 0.70;
    let badge_h = if is_multiline {
        sz * 0.18 * line_count as f32
    } else {
        sz * 0.22
    };
    let badge_x = (sz - badge_w) / 2.0;
    let badge_y = sz - badge_h - sz * 0.08;
    let badge_radius = if is_multiline {
        badge_h * 0.25
    } else {
        badge_h / 2.0
    };

    let badge_rect = make_rounded_rect(badge_x, badge_y, badge_w, badge_h, badge_radius);

    // Shadow
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    for offset in [2.0_f32, 1.0] {
        paint.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.15).unwrap());
        let sr = make_rounded_rect(badge_x, badge_y - offset, badge_w, badge_h, badge_radius);
        pixmap.fill_path(&sr, &paint, FillRule::Winding, Transform::identity(), None);
    }

    // Fill
    paint.set_color(Color::from_rgba(style.r, style.g, style.b, 0.95).unwrap());
    pixmap.fill_path(
        &badge_rect,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    // Border
    paint.set_color(
        Color::from_rgba(
            (style.r * 0.5).min(1.0),
            (style.g * 0.5).min(1.0),
            (style.b * 0.5).min(1.0),
            0.8,
        )
        .unwrap(),
    );
    let stroke = Stroke {
        width: (sz * 0.012).max(1.0),
        ..Stroke::default()
    };
    pixmap.stroke_path(&badge_rect, &paint, &stroke, Transform::identity(), None);

    // Text
    let font_size = if is_multiline {
        badge_h * 0.30
    } else {
        badge_h * 0.55
    };
    let scale = PxScale::from(font_size);
    let scaled = font.as_scaled(scale);
    let line_height = scaled.height();
    let total_h = line_height * line_count as f32;
    let start_y = badge_y + (badge_h - total_h) / 2.0 + scaled.ascent();

    for (i, line) in lines.iter().enumerate() {
        let y = start_y + i as f32 * line_height;
        draw_text_centered(pixmap, &scaled, line, sz / 2.0, y);
    }
}

fn draw_text_centered(
    pixmap: &mut Pixmap,
    font: &ab_glyph::PxScaleFont<&FontRef>,
    text: &str,
    cx: f32,
    y: f32,
) {
    let mut w = 0.0_f32;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        if let Some(p) = prev {
            w += font.kern(p, id);
        }
        w += font.h_advance(id);
        prev = Some(id);
    }

    let pw = pixmap.width();
    let ph = pixmap.height();
    let pixels = pixmap.pixels_mut();
    let mut x = cx - w / 2.0;
    prev = None;
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        if let Some(p) = prev {
            x += font.kern(p, id);
        }
        let glyph = id.with_scale_and_position(font.scale(), ab_glyph::point(x, y));
        if let Some(o) = font.outline_glyph(glyph) {
            let b = o.px_bounds();
            o.draw(|gx, gy, cov| {
                let px = b.min.x as i32 + gx as i32;
                let py = b.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && (px as u32) < pw && (py as u32) < ph {
                    let a = (cov * 255.0) as u8;
                    if a > 0 {
                        let idx = (py as u32 * pw + px as u32) as usize;
                        let e = pixels[idx];
                        let sa = a as f32 / 255.0;
                        let da = e.alpha() as f32 / 255.0;
                        let oa = sa + da * (1.0 - sa);
                        if oa > 0.0 {
                            let bl = |s: u8, d: u8| {
                                ((s as f32 * sa + d as f32 * da * (1.0 - sa)) / oa) as u8
                            };
                            pixels[idx] = ColorU8::from_rgba(
                                bl(255, e.red()),
                                bl(255, e.green()),
                                bl(255, e.blue()),
                                (oa * 255.0) as u8,
                            )
                            .premultiply();
                        }
                    }
                }
            });
        }
        x += font.h_advance(id);
        prev = Some(id);
    }
}

fn make_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
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
