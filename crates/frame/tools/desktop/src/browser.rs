//! The library browser: a grid of surface previews, drawn by frame itself
//! (a scene built in code: tiles are frames, previews are image fills).

use std::collections::BTreeMap;

use frame::document::proto::{Asset, NodeId, Scene};
use frame::library::link::{Signature, SurfaceMeta};
use frame::surface::runtime::Bounds;

/// Grid geometry, scene px.
const COLS: usize = 4;
const ROWS: usize = 3;
const TILE_W: f64 = 280.0;
const TILE_H: f64 = 214.0;
const GAP: f64 = 20.0;
const HEADER: f64 = 84.0;
const THUMB_H: f64 = 150.0;
const WIDTH: f64 = COLS as f64 * TILE_W + (COLS as f64 + 1.0) * GAP;
const HEIGHT: f64 = HEADER + ROWS as f64 * TILE_H + (ROWS as f64 + 1.0) * GAP;

pub const BACKGROUND: peniko::Color = peniko::Color::from_rgb8(0x0e, 0x0f, 0x12);

/// A browser key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
}

pub struct Browser {
    /// Where the surfaces come from, for the header.
    source: String,
    /// Only surfaces fitting this engine (`--engine`), and its name.
    fits: Option<(String, Signature)>,
    items: Vec<SurfaceMeta>,
    selected: usize,
    /// The built scene, until something changes.
    cache: Option<(Scene, NodeId)>,
}

impl Browser {
    #[must_use]
    pub fn new(source: impl Into<String>, fits: Option<(String, Signature)>) -> Self {
        Self {
            source: source.into(),
            fits,
            items: Vec::new(),
            selected: 0,
            cache: None,
        }
    }

    /// Replace the listing (filtered by the engine, if any), keeping the
    /// selection on the same surface when it's still there.
    pub fn set_items(&mut self, all: &[SurfaceMeta]) {
        let keep = self.selected().map(|m| m.id.clone());
        self.items = all
            .iter()
            .filter(|m| self.fits.as_ref().is_none_or(|(_, s)| m.fits.fits(s)))
            .cloned()
            .collect();
        self.selected = keep
            .and_then(|id| self.items.iter().position(|m| m.id == id))
            .unwrap_or(0)
            .min(self.items.len().saturating_sub(1));
        self.cache = None;
    }

    #[must_use]
    pub fn selected(&self) -> Option<&SurfaceMeta> {
        self.items.get(self.selected)
    }

    #[cfg(test)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Select the surface `id`, if listed.
    pub fn select(&mut self, id: &str) {
        if let Some(i) = self.items.iter().position(|m| m.id == id) {
            self.selected = i;
            self.cache = None;
        }
    }

    pub fn nav(&mut self, nav: Nav) {
        if self.items.is_empty() {
            return;
        }
        let last = self.items.len() - 1;
        let s = self.selected;
        self.selected = match nav {
            Nav::Left => s.saturating_sub(1),
            Nav::Right => (s + 1).min(last),
            Nav::Up => s.saturating_sub(COLS),
            Nav::Down => (s + COLS).min(last),
            Nav::PageUp => s.saturating_sub(COLS * ROWS),
            Nav::PageDown => (s + COLS * ROWS).min(last),
        };
        if self.selected != s {
            self.cache = None;
        }
    }

    /// The whole grid's box (what the window fits).
    #[must_use]
    pub fn content(&self) -> Bounds {
        Bounds {
            x: 0.0,
            y: 0.0,
            width: WIDTH,
            height: HEIGHT,
        }
    }

    /// The first row shown: pages of `ROWS` rows, following the selection.
    fn first_row(&self) -> usize {
        (self.selected / COLS) / ROWS * ROWS
    }

    fn tile_origin(slot: usize) -> (f64, f64) {
        let (r, c) = (slot / COLS, slot % COLS);
        (
            GAP + c as f64 * (TILE_W + GAP),
            HEADER + GAP + r as f64 * (TILE_H + GAP),
        )
    }

    /// The tile under a point (scene px), as an index into the listing.
    #[must_use]
    pub fn hit(&self, x: f64, y: f64) -> Option<usize> {
        let first = self.first_row() * COLS;
        (0..COLS * ROWS).find_map(|slot| {
            let (tx, ty) = Self::tile_origin(slot);
            let i = first + slot;
            (i < self.items.len() && x >= tx && x <= tx + TILE_W && y >= ty && y <= ty + TILE_H)
                .then_some(i)
        })
    }

    /// Select tile `i` (a click).
    pub fn set_selected(&mut self, i: usize) {
        if i < self.items.len() && i != self.selected {
            self.selected = i;
            self.cache = None;
        }
    }

    /// The grid as a frame scene.
    pub fn scene(&mut self) -> &(Scene, NodeId) {
        if self.cache.is_none() {
            self.cache = Some(self.build());
        }
        self.cache.as_ref().expect("built above")
    }

    fn build(&self) -> (Scene, NodeId) {
        let q = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
        let mut assets = BTreeMap::new();
        let pages = self.items.len().div_ceil(COLS * ROWS).max(1);
        let page = self.first_row() / ROWS + 1;
        let filter = self
            .fits
            .as_ref()
            .map_or_else(String::new, |(name, _)| format!(" · fits {name}"));
        let mut tiles = String::new();
        let first = self.first_row() * COLS;
        for (slot, meta) in self
            .items
            .iter()
            .enumerate()
            .skip(first)
            .take(COLS * ROWS)
            .map(|(i, m)| (i - first, m))
        {
            let (x, y) = Self::tile_origin(slot);
            let selected = first + slot == self.selected;
            let stroke = if selected {
                r##"stroke="#4f8cff" stroke-weight=3 stroke-align="inside""##
            } else {
                r##"stroke="#ffffff14""##
            };
            let thumb = if meta.preview_png.is_empty() {
                format!(
                    r##"<Frame name="thumb" x=10 y=10 w={w} h={THUMB_H} radius=6 fill="#23252b"><Text name="placeholder" x=0 y=64 w={w} h=20 size=13 text-align="center" fill="#6b6f7a" text="no preview" /></Frame>"##,
                    w = TILE_W - 20.0
                )
            } else {
                let asset = Asset::new("image/png", meta.preview_png.clone());
                let id = asset.id.clone();
                assets.insert(id.clone(), asset);
                // The image sized to its aspect inside the thumb well, centered.
                let (bw, bh) = (TILE_W - 20.0, THUMB_H);
                let (iw, ih) = png_size(&meta.preview_png).map_or((bw, bh), |(w, h)| {
                    let k = (bw / w).min(bh / h);
                    (w * k, h * k)
                });
                format!(
                    r##"<Frame name="thumb" x=10 y=10 w={bw} h={bh} radius=6 clip fill="#08090b"><Rectangle name="preview" x={ix} y={iy} w={iw} h={ih} fill={{[{{"kind":{{"Image":{{"asset":"{id}","scale_mode":"Stretch"}}}}}}]}} /></Frame>"##,
                    ix = (bw - iw) / 2.0,
                    iy = (bh - ih) / 2.0,
                    id = id.0,
                )
            };
            let sub = format!(
                "{} · v{}{}",
                meta.tags.join(" · "),
                meta.version,
                meta.forked_from
                    .as_ref()
                    .map_or(String::new(), |f| format!(" · fork of {f}"))
            );
            tiles += &format!(
                r##"<Frame name={name} x={x} y={y} w={TILE_W} h={TILE_H} radius=10 fill="{fill}" {stroke}>{thumb}<Text name="title" x=12 y={ty} w={tw} h=20 size=15 fill="#f2f3f5" text={title} /><Text name="sub" x=12 y={sy} w={tw} h=16 size=11 fill="#8a8f9c" text={sub} /></Frame>"##,
                name = q(&meta.id),
                fill = if selected { "#1d2230" } else { "#17181c" },
                ty = THUMB_H + 20.0,
                sy = THUMB_H + 44.0,
                tw = TILE_W - 24.0,
                title = q(&meta.name),
                sub = q(&sub),
            );
        }
        if self.items.is_empty() {
            tiles += &format!(
                r##"<Text name="empty" x={GAP} y={y} w={w} h=24 size=16 fill="#8a8f9c" text={t} />"##,
                y = HEADER + 40.0,
                w = WIDTH - 2.0 * GAP,
                t = q(if self.fits.is_some() {
                    "No surfaces in the library fit this engine."
                } else {
                    "The library is empty."
                }),
            );
        }
        let selected = self
            .selected()
            .map_or_else(String::new, |m| format!(" · {}", m.id));
        let src = format!(
            r##"<Document><Page name="Library"><Frame name="Browser" w={WIDTH} h={HEIGHT} fill="#0e0f12">
<Text name="heading" x={GAP} y=18 w=600 h=28 size=22 weight=600 fill="#f2f3f5" text="Surface library" />
<Text name="status" x={GAP} y=52 w={sw} h=16 size=12 fill="#8a8f9c" text={status} />
<Text name="keys" x={kx} y=24 w=560 h=16 size=12 text-align="right" fill="#6b6f7a" text={keys} />
{tiles}</Frame></Page></Document>"##,
            sw = WIDTH - 2.0 * GAP,
            kx = WIDTH - GAP - 560.0,
            status = q(&format!(
                "{} surfaces · {}{filter} · page {page}/{pages}{selected}",
                self.items.len(),
                self.source
            )),
            keys = q("arrows select · Enter preview · I install · F fork · B back · Esc quit"),
        );
        let scene = match frame::document::text::parse(&src, assets) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("frame: browser scene: {e}");
                Scene::new("Library")
            }
        };
        let page = scene.pages()[0];
        (scene, page)
    }
}

/// A PNG's pixel size, from its IHDR.
fn png_size(png: &[u8]) -> Option<(f64, f64)> {
    let be = |at: usize| {
        png.get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let (w, h) = (be(16)?, be(20)?);
    (png.starts_with(b"\x89PNG") && w > 0 && h > 0).then(|| (f64::from(w), f64::from(h)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str, params: &[&str]) -> SurfaceMeta {
        SurfaceMeta {
            id: id.into(),
            name: id.to_uppercase(),
            version: 1,
            fits: Signature {
                params: params.iter().map(|s| (*s).to_owned()).collect(),
                ..Signature::default()
            },
            ..SurfaceMeta::default()
        }
    }

    fn many(n: usize) -> Vec<SurfaceMeta> {
        (0..n).map(|i| meta(&format!("s{i:02}"), &[])).collect()
    }

    #[test]
    fn builds_a_tile_per_visible_surface() {
        let mut b = Browser::new("ws://x", None);
        b.set_items(&many(5));
        let (scene, page) = b.scene();
        let root = scene.node(*page).unwrap().children[0];
        let tiles = scene
            .node(root)
            .unwrap()
            .children
            .iter()
            .filter(|id| scene.node(**id).is_some_and(|n| n.name.starts_with("s0")))
            .count();
        assert_eq!(tiles, 5);
    }

    #[test]
    fn previews_become_image_fills() {
        let png = {
            let img = image::RgbaImage::from_pixel(4, 2, image::Rgba([200, 10, 10, 255]));
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        let mut b = Browser::new("ws://x", None);
        b.set_items(&[SurfaceMeta {
            preview_png: png,
            ..meta("a", &[])
        }]);
        let (scene, _) = b.scene();
        assert_eq!(scene.assets.len(), 1);
        let preview = scene.nodes.values().find(|n| n.name == "preview").unwrap();
        assert_eq!(
            (preview.size.width, preview.size.height),
            (260.0, 130.0),
            "2:1 fitted into the well"
        );
    }

    #[test]
    fn navigation_pages_and_clamps() {
        let mut b = Browser::new("ws://x", None);
        b.set_items(&many(30));
        b.nav(Nav::Down);
        assert_eq!(b.selected().unwrap().id, "s04");
        b.nav(Nav::PageDown);
        assert_eq!(b.selected().unwrap().id, "s16");
        assert_eq!(b.first_row(), 3, "second page");
        // The first tile of that page is where the first slot is drawn.
        let (x, y) = Browser::tile_origin(0);
        assert_eq!(b.hit(x + 5.0, y + 5.0), Some(12));
        b.nav(Nav::PageDown);
        b.nav(Nav::PageDown);
        assert_eq!(b.selected().unwrap().id, "s29", "clamped to the last");
        b.nav(Nav::Right);
        assert_eq!(b.selected().unwrap().id, "s29");
        assert_eq!(b.hit(1.0, 1.0), None, "the header isn't a tile");
    }

    #[test]
    fn fits_filter_and_selection_survive_a_refresh() {
        let engine = Signature {
            params: vec!["a".into(), "b".into()],
            ..Signature::default()
        };
        let mut b = Browser::new("ws://x", Some(("demo".into(), engine)));
        let all = vec![meta("x", &["a"]), meta("y", &["z"]), meta("z", &["b"])];
        b.set_items(&all);
        assert_eq!(b.len(), 2, "y needs a param the engine lacks");
        b.select("z");
        b.set_items(&[meta("w", &[]), meta("x", &["a"]), meta("z", &["b"])]);
        assert_eq!(b.selected().unwrap().id, "z");
    }
}
