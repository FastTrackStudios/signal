//! Theme image catalog + atlas slicers.
//!
//! REAPER theme images encode geometry with **pink `RGB(255,0,255)` marker
//! lines** along the 1px image edges (see `reaper-theme/docs/theme-images.md`
//! §3, verified against the Anti-Theme):
//!
//! - the **top** edge line carries a left-anchored pink run = the *fixed left*
//!   region; the **bottom** edge a right-anchored run = *fixed right*;
//! - the **left** edge a top-anchored run = *fixed top*; the **right** edge a
//!   bottom-anchored run = *fixed bottom* (a single line may carry both runs);
//! - marker lines are not part of the rendered content and are stripped;
//! - yellow `RGB(255,255,0)` marks outer extents (treated as marker too).
//!
//! 3-slice buttons are `normal | mouseover | pressed` left→right; with a
//! marker ring the content width is `(w − 2)` (e.g. Anti-Theme `mcp_io`
//! 62×34 → 3 × 20×32 states).

use crate::ThemeError;
use image::{GenericImageView, RgbaImage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Pink/yellow marker detection.
fn is_marker(px: image::Rgba<u8>) -> bool {
    let [r, g, b, _] = px.0;
    (r == 255 && g == 0 && b == 255) || (r == 255 && g == 255 && b == 0)
}

/// Pink only — the stretch-geometry colour. Yellow marks outer extents and
/// is stripped like a marker line but carries no fixed-margin meaning.
fn is_pink(px: image::Rgba<u8>) -> bool {
    let [r, g, b, _] = px.0;
    r == 255 && g == 0 && b == 255
}

/// Stretch-geometry margins decoded from the pink marker lines (px, relative
/// to the *content* image, i.e. after marker lines are stripped).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Markers {
    pub fixed_left: u32,
    pub fixed_top: u32,
    pub fixed_right: u32,
    pub fixed_bottom: u32,
}

/// An image with its marker lines stripped + decoded margins.
#[derive(Clone, Debug)]
pub struct Sliced {
    pub image: RgbaImage,
    pub markers: Markers,
}

/// A 3-slice button: normal / mouseover / pressed, plus the pink-line
/// stretch margins (shared by all three states — the marker corners sit at
/// the normal state's top-left and the pressed state's bottom-right).
#[derive(Clone, Debug)]
pub struct Slice3 {
    pub normal: RgbaImage,
    pub hover: RgbaImage,
    pub pressed: RgbaImage,
    pub markers: Markers,
}

/// A knob filmstrip: `frames` square-ish frames stacked along the long axis.
#[derive(Clone, Debug)]
pub struct KnobStack {
    pub image: RgbaImage,
    pub frames: u32,
    pub frame_w: u32,
    pub frame_h: u32,
    /// Frames run vertically (true) or horizontally.
    pub vertical: bool,
}

/// The theme folder's PNG vocabulary, by image name (file stem).
#[derive(Clone)]
pub struct ImageCatalog {
    dir: PathBuf,
    names: HashMap<String, PathBuf>,
}

impl ImageCatalog {
    /// An empty catalog with no images and no backing directory. For
    /// in-memory themes on platforms without a filesystem (wasm): every
    /// [`load`](Self::load) misses, so image skins resolve to `None` and the
    /// UI falls back to vector rendering. Palette + rtconfig + WALTER layout
    /// still work (they don't touch images).
    pub fn empty() -> Self {
        Self {
            dir: PathBuf::new(),
            names: HashMap::new(),
        }
    }

    /// Scan a theme image folder (non-recursive; per-DPI subfolders are a
    /// later phase).
    pub fn scan(dir: &Path) -> Result<Self, ThemeError> {
        let mut names = HashMap::new();
        let entries = std::fs::read_dir(dir).map_err(|source| ThemeError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                names.insert(stem.to_string(), path);
            }
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            names,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Overlay a per-DPI image subfolder (`150/`, `200/` — selected via the
    /// rtconfig `misc_dpi_translate` table): its images override the base
    /// names; everything else falls back to the 100% set.
    pub fn overlay_subdir(&mut self, sub: &str) -> Result<(), ThemeError> {
        let dir = self.dir.join(sub);
        let entries = std::fs::read_dir(&dir).map_err(|source| ThemeError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                self.names.insert(stem.to_string(), path);
            }
        }
        Ok(())
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names.keys().map(|s| s.as_str())
    }

    /// Decode an image by name (raw, markers included).
    pub fn load_raw(&self, name: &str) -> Result<RgbaImage, ThemeError> {
        let path = self
            .names
            .get(name)
            .ok_or_else(|| ThemeError::Io {
                path: self.dir.join(format!("{name}.png")),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such theme image"),
            })?
            .clone();
        let img = image::open(&path)
            .map_err(|source| ThemeError::Image { path, source })?
            .to_rgba8();
        Ok(img)
    }

    /// Decode an image and strip/decode its pink marker lines.
    pub fn load(&self, name: &str) -> Result<Sliced, ThemeError> {
        Ok(strip_markers(self.load_raw(name)?))
    }

    /// Slice a 3-state button image (`normal|hover|pressed` left→right).
    /// Width divides by 3 with the remainder ignored — overlay (`*_ol`)
    /// variants pad a pixel or two beyond exact thirds.
    pub fn button3(&self, name: &str) -> Result<Slice3, ThemeError> {
        let sliced = self.load(name)?;
        let img = sliced.image;
        let (w, h) = img.dimensions();
        if w < 3 {
            return Err(ThemeError::BadGeometry {
                path: self.dir.join(format!("{name}.png")),
                geometry: "3-slice button",
                width: w,
                height: h,
            });
        }
        let sw = w / 3;
        let crop = |i: u32| img.view(i * sw, 0, sw, h).to_image();
        Ok(Slice3 {
            normal: crop(0),
            hover: crop(1),
            pressed: crop(2),
            markers: sliced.markers,
        })
    }

    /// Decode a knob filmstrip: frames are square, stacked along the long
    /// axis (`tcp_pan_knob_stack` 20×820 → 41 frames of 20×20).
    pub fn knob_stack(&self, name: &str) -> Result<KnobStack, ThemeError> {
        let sliced = self.load(name)?;
        let img = sliced.image;
        let (w, h) = img.dimensions();
        let (frames, frame_w, frame_h, vertical) = if h >= w && w > 0 && h % w == 0 {
            (h / w, w, w, true)
        } else if w > h && h > 0 && w % h == 0 {
            (w / h, h, h, false)
        } else {
            return Err(ThemeError::BadGeometry {
                path: self.dir.join(format!("{name}.png")),
                geometry: "knob filmstrip",
                width: w,
                height: h,
            });
        };
        Ok(KnobStack {
            image: img,
            frames,
            frame_w,
            frame_h,
            vertical,
        })
    }

    /// Encode an RGBA image back to PNG bytes (for data-URI delivery).
    pub fn encode_png(img: &RgbaImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        // Encoding an in-memory RGBA image to PNG cannot fail.
        image::write_buffer_with_format(
            &mut out,
            img.as_raw(),
            img.width(),
            img.height(),
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .expect("in-memory png encode");
        out.into_inner()
    }

    /// Encode an RGBA image as a `data:image/png;base64,…` URI.
    pub fn data_uri(img: &RgbaImage) -> String {
        format!("data:image/png;base64,{}", base64(&Self::encode_png(img)))
    }
}

/// Alpha-composite `top` over `base`, both anchored top-left, on a canvas
/// covering both. Used to merge `name_ol.png` button overlays (the visible
/// art in `use_overlays 1` themes — the default theme's button bases are
/// fully transparent) onto their base states.
pub fn alpha_over(base: &RgbaImage, top: &RgbaImage) -> RgbaImage {
    let w = base.width().max(top.width());
    let h = base.height().max(top.height());
    let mut out = RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 0]));
    image::imageops::overlay(&mut out, base, 0, 0);
    image::imageops::overlay(&mut out, top, 0, 0);
    out
}

/// Minimal standard-alphabet base64 (padding included) — avoids a dependency
/// for the one data-URI use.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        let enc = |shift: u32| ABC[((n >> shift) & 0x3f) as usize] as char;
        out.push(enc(18));
        out.push(enc(12));
        out.push(if chunk.len() > 1 { enc(6) } else { '=' });
        out.push(if chunk.len() > 2 { enc(0) } else { '=' });
    }
    out
}

/// Detect + strip pink/yellow marker edge lines, decoding the fixed-region
/// margins they describe.
pub fn strip_markers(img: RgbaImage) -> Sliced {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return Sliced {
            image: img,
            markers: Markers::default(),
        };
    }

    // An edge is a marker line iff its *interior* (corners excluded) holds
    // marker pixels, or it owns an *isolated* marker corner — one whose
    // neighbour along the perpendicular edge is not a marker (otherwise the
    // corner belongs to that perpendicular line's run). Verified against the
    // Anti-Theme: `mcp_volbg` (full ring with runs), `mcp_volthumb` (right
    // line only; its corners are the line's own run ends), `mcp_io` (left
    // run + a lone lower-right corner marking bottom+right).
    let mk = |x: u32, y: u32| is_marker(*img.get_pixel(x, y));
    let interior_h = |y: u32| (1..w - 1).any(|x| mk(x, y));
    let interior_v = |x: u32| (1..h - 1).any(|y| mk(x, y));
    // corner (x,y) counts toward a horizontal edge iff not part of a vertical run.
    let corner_owns_h = |x: u32, y: u32| mk(x, y) && !mk(x, if y == 0 { 1 } else { h - 2 });
    let corner_owns_v = |x: u32, y: u32| mk(x, y) && !mk(if x == 0 { 1 } else { w - 2 }, y);

    let top = interior_h(0) || corner_owns_h(0, 0) || corner_owns_h(w - 1, 0);
    let bottom = interior_h(h - 1) || corner_owns_h(0, h - 1) || corner_owns_h(w - 1, h - 1);
    let left = interior_v(0) || corner_owns_v(0, 0) || corner_owns_v(0, h - 1);
    let right = interior_v(w - 1) || corner_owns_v(w - 1, 0) || corner_owns_v(w - 1, h - 1);

    // Content rect after stripping marker lines.
    let cx = u32::from(left);
    let cy = u32::from(top);
    let cw = w - cx - u32::from(right);
    let ch = h - cy - u32::from(bottom);

    // Run lengths measured over the content span (corner pixels excluded).
    let run_from_start = |horiz: bool, idx: u32| -> u32 {
        let span = if horiz { cx..cx + cw } else { cy..cy + ch };
        let mut n = 0;
        for i in span {
            let px = if horiz {
                img.get_pixel(i, idx)
            } else {
                img.get_pixel(idx, i)
            };
            if is_marker(*px) {
                n += 1;
            } else {
                break;
            }
        }
        n
    };
    let run_from_end = |horiz: bool, idx: u32| -> u32 {
        let span = if horiz { cx..cx + cw } else { cy..cy + ch };
        let mut n = 0;
        for i in span.rev() {
            let px = if horiz {
                img.get_pixel(i, idx)
            } else {
                img.get_pixel(idx, i)
            };
            if is_marker(*px) {
                n += 1;
            } else {
                break;
            }
        }
        n
    };

    // A pink run on a line whose corners are *completely unmarked* marks
    // the STRETCH zone instead of the fixed margins (the SDK's scrollbar
    // convention: "the pink sections determine how each slice will be
    // stretched; the areas delimited by the pink color will not be
    // stretched" — `scrollbar.png` is the only corpus image using it).
    // Corners count as anchored when they hold *either* marker colour:
    // yellow corners accompany pink runs on ordinary fixed-margin art
    // (`tcp_labelBlock_bg` = `YPPP…`, `mcp_recinput` = `YPP…`), so testing
    // pink-only mis-routes them here and warps the fixed caps.
    // Returns the run as `(offset, len)` in content coordinates.
    let interior_run = |horiz: bool, idx: u32| -> Option<(u32, u32)> {
        let span: Vec<u32> = if horiz {
            (cx..cx + cw).collect()
        } else {
            (cy..cy + ch).collect()
        };
        let pink_at = |i: u32| {
            let px = if horiz {
                img.get_pixel(i, idx)
            } else {
                img.get_pixel(idx, i)
            };
            is_pink(*px)
        };
        let start = span.iter().position(|&i| pink_at(i))?;
        let len = span[start..].iter().take_while(|&&i| pink_at(i)).count() as u32;
        Some((start as u32, len))
    };

    let mut m = Markers::default();
    // Horizontal regions: corner-anchored pink runs mark fixed margins
    // (top line left-anchored, bottom line right-anchored; a single line
    // may carry both); a non-anchored run marks the stretch zone.
    if top {
        let anchored_l = is_marker(*img.get_pixel(0, 0));
        let anchored_r = is_marker(*img.get_pixel(w - 1, 0));
        if anchored_l {
            m.fixed_left = run_from_start(true, 0);
        }
        if anchored_r && !bottom {
            m.fixed_right = run_from_end(true, 0);
        }
        if !anchored_l
            && !anchored_r
            && let Some((a, len)) = interior_run(true, 0)
        {
            m.fixed_left = a;
            m.fixed_right = cw.saturating_sub(a + len);
        }
    }
    if bottom {
        let anchored_r = is_marker(*img.get_pixel(w - 1, h - 1));
        let anchored_l = is_marker(*img.get_pixel(0, h - 1));
        if anchored_r {
            m.fixed_right = run_from_end(true, h - 1);
        }
        if anchored_l && !top {
            m.fixed_left = run_from_start(true, h - 1);
        }
        if !anchored_l
            && !anchored_r
            && !top
            && let Some((a, len)) = interior_run(true, h - 1)
        {
            m.fixed_left = a;
            m.fixed_right = cw.saturating_sub(a + len);
        }
    }
    // Vertical regions: left col top-anchored, right col bottom-anchored.
    if left {
        let anchored_t = is_marker(*img.get_pixel(0, 0));
        let anchored_b = is_marker(*img.get_pixel(0, h - 1));
        if anchored_t {
            m.fixed_top = run_from_start(false, 0);
        }
        if anchored_b && !right {
            m.fixed_bottom = run_from_end(false, 0);
        }
        if !anchored_t
            && !anchored_b
            && let Some((a, len)) = interior_run(false, 0)
        {
            m.fixed_top = a;
            m.fixed_bottom = ch.saturating_sub(a + len);
        }
    }
    if right {
        let anchored_b = is_marker(*img.get_pixel(w - 1, h - 1));
        let anchored_t = is_marker(*img.get_pixel(w - 1, 0));
        if anchored_b {
            m.fixed_bottom = run_from_end(false, w - 1);
        }
        if anchored_t && !left {
            m.fixed_top = run_from_start(false, w - 1);
        }
        if !anchored_t
            && !anchored_b
            && !left
            && let Some((a, len)) = interior_run(false, w - 1)
        {
            m.fixed_top = a;
            m.fixed_bottom = ch.saturating_sub(a + len);
        }
    }

    let content = img.view(cx, cy, cw, ch).to_image();
    Sliced {
        image: content,
        markers: m,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const PINK: Rgba<u8> = Rgba([255, 0, 255, 255]);
    const GREY: Rgba<u8> = Rgba([40, 40, 40, 255]);

    fn img(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, GREY)
    }

    #[test]
    fn base64_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn no_markers_passthrough() {
        let s = strip_markers(img(60, 20));
        assert_eq!(s.image.dimensions(), (60, 20));
        assert_eq!(s.markers, Markers::default());
    }

    #[test]
    fn full_ring_margins_decode() {
        // Mimic mcp_volbg: 26×22 ring; top run 12 from left, bottom run 12
        // from right, left run 10 from top, right run 10 from bottom.
        let mut i = img(26, 22);
        for x in 0..12 {
            i.put_pixel(x, 0, PINK);
        }
        for x in 14..26 {
            i.put_pixel(x, 21, PINK);
        }
        for y in 0..10 {
            i.put_pixel(0, y, PINK);
        }
        for y in 12..22 {
            i.put_pixel(25, y, PINK);
        }
        let s = strip_markers(i);
        assert_eq!(s.image.dimensions(), (24, 20));
        assert_eq!(
            s.markers,
            Markers {
                // Runs measured over the content span: corner pixel excluded.
                fixed_left: 11,
                fixed_right: 11,
                fixed_top: 9,
                fixed_bottom: 9,
            }
        );
    }

    #[test]
    fn single_right_line_carries_both_runs() {
        // Mimic mcp_volthumb: only the right column is a marker line, with
        // top- and bottom-anchored runs of 5.
        let mut i = img(24, 53);
        for y in 0..5 {
            i.put_pixel(23, y, PINK);
        }
        for y in 48..53 {
            i.put_pixel(23, y, PINK);
        }
        let s = strip_markers(i);
        assert_eq!(s.image.dimensions(), (23, 53));
        assert_eq!(s.markers.fixed_top, 5);
        assert_eq!(s.markers.fixed_bottom, 5);
        assert_eq!(s.markers.fixed_left, 0);
    }

    #[test]
    fn button3_plain_thirds() {
        let mut i = img(60, 20);
        // Distinguish the states by a corner pixel each.
        i.put_pixel(0, 5, PINK_FREE_RED);
        i.put_pixel(20, 5, PINK_FREE_GREEN);
        i.put_pixel(40, 5, PINK_FREE_BLUE);
        let dir = std::env::temp_dir().join("daw-theme-reaper-test-btn");
        std::fs::create_dir_all(&dir).unwrap();
        i.save(dir.join("btn.png")).unwrap();
        let cat = ImageCatalog::scan(&dir).unwrap();
        let s = cat.button3("btn").unwrap();
        assert_eq!(s.normal.dimensions(), (20, 20));
        assert_eq!(*s.normal.get_pixel(0, 5), PINK_FREE_RED);
        assert_eq!(*s.hover.get_pixel(0, 5), PINK_FREE_GREEN);
        assert_eq!(*s.pressed.get_pixel(0, 5), PINK_FREE_BLUE);
    }

    const PINK_FREE_RED: Rgba<u8> = Rgba([200, 10, 10, 255]);
    const PINK_FREE_GREEN: Rgba<u8> = Rgba([10, 200, 10, 255]);
    const PINK_FREE_BLUE: Rgba<u8> = Rgba([10, 10, 200, 255]);
}

#[cfg(test)]
mod marker_semantics_tests {
    use super::*;

    /// `tcp_labelBlock_bg` (`YPPP…` top, `…PPPYYY` bottom): yellow-anchored
    /// pink runs are FIXED margins — the capsule's rounded caps must not
    /// stretch (a pink-only corner test warped the left cap).
    #[test]
    fn yellow_anchored_runs_are_fixed_margins() {
        let Ok(theme) = crate::ReaperTheme::load_dir(
            "/home/cody/Development/FastTrackStudio/reaper-theme/extracted/antitheme",
        ) else {
            eprintln!("anti-theme not found — skipping");
            return;
        };
        let cat = &theme.images;
        let s = cat.load("tcp_labelBlock_bg").expect("labelBlock loads");
        assert_eq!(s.markers.fixed_left, 12, "left cap fixed (12P run)");
        assert_eq!(s.markers.fixed_right, 12, "right cap fixed (P run + Y)");

        // `mcp_recinput`: fixed margins pin the dropdown arrow (it lives in
        // the bottom-right fixed region) — only the small text gap stretches.
        let s = cat.load("mcp_recinput").expect("recinput loads");
        assert_eq!(s.markers.fixed_left, 2);
        assert_eq!(s.markers.fixed_right, 18);
        assert!(s.markers.fixed_bottom >= 18);
    }

    /// Completely unmarked corners + an interior pink run = the SDK's
    /// scrollbar stretch-zone convention: the run stretches, the rest fixes.
    #[test]
    fn unmarked_corner_interior_run_is_stretch_zone() {
        // 12x4: top line with pink at x=4..6 only, corners empty.
        let mut img = RgbaImage::from_pixel(12, 4, image::Rgba([10, 10, 10, 255]));
        for x in 4..7 {
            img.put_pixel(x, 0, image::Rgba([255, 0, 255, 255]));
        }
        let s = strip_markers(img);
        // Content is 12 wide (no left/right lines), 3 tall (top stripped).
        assert_eq!(s.markers.fixed_left, 4);
        assert_eq!(s.markers.fixed_right, 12 - (4 + 3));
    }
}
