//! Turning a PNG into rects.
//!
//! One rect per pixel would be correct and enormous — a 63×20 button is
//! 1260 of them, and there are a thousand images. So runs of identical
//! pixels are merged horizontally, then vertically where whole runs line
//! up, which typically cuts it by an order of magnitude on theme art
//! (large flat areas, long 1px bevels).
//!
//! Fully transparent pixels are dropped rather than emitted as clear rects:
//! they are most of a nine-slice source, and they draw nothing.
//!
//! Marker pixels are dropped too — they are stamped back verbatim after
//! rasterising, so tracing them would double-draw the guides and, worse,
//! subject them to the colour ramp.

use image::RgbaImage;

use crate::art_data::Rect;
use crate::derive::MARKERS;

/// Trace an image into merged rects.
pub fn trace(img: &RgbaImage) -> Vec<Rect> {
    let (w, h) = (img.width(), img.height());

    // Horizontal runs per row.
    let mut runs: Vec<Rect> = Vec::new();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            let px = img.get_pixel(x, y).0;
            if px[3] == 0 || MARKERS.contains(&px) {
                x += 1;
                continue;
            }
            let start = x;
            while x + 1 < w && img.get_pixel(x + 1, y).0 == px {
                x += 1;
            }
            runs.push(Rect {
                x: start as u16,
                y: y as u16,
                w: (x - start + 1) as u16,
                h: 1,
                rgba: px,
            });
            x += 1;
        }
    }

    // Merge vertically: a run directly below one of the same x/width/colour
    // extends it. Theme art is full of tall flat columns, so this is where
    // most of the reduction comes from.
    let mut merged: Vec<Rect> = Vec::new();
    for run in runs {
        if let Some(prev) = merged
            .iter_mut()
            .rev()
            .find(|p| p.x == run.x && p.w == run.w && p.rgba == run.rgba && p.y + p.h == run.y)
        {
            prev.h += 1;
        } else {
            merged.push(run);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| Rgba(f(x, y)))
    }

    #[test]
    fn a_flat_block_becomes_one_rect() {
        let a = img(4, 4, |_, _| [40, 40, 40, 255]);
        let rects = trace(&a);
        assert_eq!(rects.len(), 1, "{rects:?}");
        assert_eq!((rects[0].w, rects[0].h), (4, 4));
    }

    #[test]
    fn transparent_pixels_are_dropped() {
        // Most of a nine-slice source is transparent; emitting clear rects
        // would multiply the output for nothing drawn.
        let a = img(4, 4, |x, _| {
            if x == 0 {
                [40, 40, 40, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let rects = trace(&a);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].w, 1);
    }

    #[test]
    fn markers_are_not_traced() {
        // They are stamped back verbatim after rasterising. Tracing them
        // would double-draw the guides and put them through the colour
        // ramp, which is exactly how a marker stops being pure magenta.
        let a = img(3, 1, |x, _| {
            if x == 1 {
                [80, 80, 80, 255]
            } else {
                [255, 0, 255, 255]
            }
        });
        let rects = trace(&a);
        assert_eq!(rects.len(), 1, "{rects:?}");
        assert_eq!(rects[0].rgba, [80, 80, 80, 255]);
    }

    #[test]
    fn a_column_merges_vertically() {
        let a = img(1, 8, |_, _| [40, 40, 40, 255]);
        let rects = trace(&a);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].h, 8);
    }

    #[test]
    fn a_checkerboard_does_not_merge_wrongly() {
        // The pathological case: merging must not span differing colours.
        let a = img(4, 4, |x, y| {
            if (x + y) % 2 == 0 {
                [10, 10, 10, 255]
            } else {
                [200, 200, 200, 255]
            }
        });
        let rects = trace(&a);
        assert_eq!(rects.len(), 16, "merged across colours: {}", rects.len());
    }

    #[test]
    fn tracing_reproduces_the_source_exactly() {
        // The property that matters: replay the rects and get the original
        // back. Without this a "1:1" score could be measuring a tracer bug.
        let src = img(9, 7, |x, y| match (x + y * 3) % 4 {
            0 => [20, 20, 20, 255],
            1 => [90, 90, 90, 255],
            2 => [0, 0, 0, 0],
            _ => [200, 200, 200, 128],
        });
        let mut replay = RgbaImage::new(9, 7);
        for r in trace(&src) {
            for dy in 0..r.h {
                for dx in 0..r.w {
                    replay.put_pixel((r.x + dx) as u32, (r.y + dy) as u32, Rgba(r.rgba));
                }
            }
        }
        assert_eq!(replay, src, "trace did not round-trip");
    }

    #[test]
    fn merging_actually_reduces_the_output() {
        // If this ever stops holding, the generated tables explode.
        let a = img(40, 40, |_, _| [40, 40, 40, 255]);
        assert!(trace(&a).len() < 40 * 40 / 100);
    }
}
