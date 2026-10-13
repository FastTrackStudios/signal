//! How far a themed mixer is from the painters' one, as numbers and a
//! picture.
//!
//! ```sh
//! cargo run -r -p session-daw --bin theme_diff -- native.png themed.png diff.png [x0,y0,x1,y1]
//! ```
//!
//! Prints the share of pixels that differ by more than a rounding
//! error and the mean difference, over the whole image or the given
//! region; writes a picture of where they differ (red, brighter is
//! further) over a dimmed copy of the native one. `just theme-parity`
//! shoots both and runs this.

use image::{Rgba, RgbaImage};

/// A channel difference under this is antialiasing noise, not a change.
const NOISE: u8 = 6;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [native, themed, out, rest @ ..] = args.as_slice() else {
        eprintln!("usage: theme_diff native.png themed.png diff.png [x0,y0,x1,y1]");
        std::process::exit(2);
    };
    let open = |p: &str| {
        image::open(p)
            .unwrap_or_else(|e| panic!("{p}: {e}"))
            .to_rgba8()
    };
    let (a, b) = (open(native), open(themed));
    assert_eq!(
        a.dimensions(),
        b.dimensions(),
        "the two shots are different sizes"
    );
    let (w, h) = a.dimensions();
    let region: Vec<u32> = rest.first().map_or_else(Vec::new, |r| {
        r.split(',').filter_map(|v| v.trim().parse().ok()).collect()
    });
    let (x0, y0, x1, y1) = match region.as_slice() {
        [x0, y0, x1, y1] => (*x0, *y0, (*x1).min(w), (*y1).min(h)),
        _ => (0, 0, w, h),
    };

    let mut diff = RgbaImage::new(w, h);
    let (mut differing, mut total, mut sum) = (0_u64, 0_u64, 0_u64);
    for y in 0..h {
        for x in 0..w {
            let (p, q) = (a.get_pixel(x, y), b.get_pixel(x, y));
            let d = (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0);
            let inside = (x0..x1).contains(&x) && (y0..y1).contains(&y);
            if inside {
                total += 1;
                sum += u64::from(d);
                if d > NOISE {
                    differing += 1;
                }
            }
            let dim = |v: u8| v / 4;
            diff.put_pixel(
                x,
                y,
                if d > NOISE && inside {
                    Rgba([d.saturating_mul(2).max(96), 0, 0, 255])
                } else {
                    Rgba([dim(p[0]), dim(p[1]), dim(p[2]), 255])
                },
            );
        }
    }
    diff.save(out).unwrap_or_else(|e| panic!("{out}: {e}"));
    // `THEME_DIFF_CROP=x0,y0,x1,y1` (with `THEME_DIFF_ZOOM`, default 8):
    // that region of the native shot, the themed one and the difference,
    // side by side and blown up, to `<out>.crop.png`.
    if let Some(crop) = std::env::var("THEME_DIFF_CROP").ok().map(|c| {
        c.split(',')
            .filter_map(|v| v.trim().parse::<u32>().ok())
            .collect::<Vec<_>>()
    }) && let [cx0, cy0, cx1, cy1] = crop.as_slice()
    {
        let zoom: u32 = std::env::var("THEME_DIFF_ZOOM")
            .ok()
            .and_then(|z| z.parse().ok())
            .unwrap_or(8);
        let (cw, ch) = (cx1.saturating_sub(*cx0), cy1.saturating_sub(*cy0));
        let mut sheet = RgbaImage::new((cw * 3 + 2) * zoom, ch * zoom);
        for (panel, source) in [&a, &b, &diff].into_iter().enumerate() {
            let left = u32::try_from(panel).unwrap_or(0) * (cw + 1) * zoom;
            for y in 0..ch * zoom {
                for x in 0..cw * zoom {
                    let p =
                        *source.get_pixel((cx0 + x / zoom).min(w - 1), (cy0 + y / zoom).min(h - 1));
                    sheet.put_pixel(left + x, y, p);
                }
            }
        }
        let path = format!("{out}.crop.png");
        sheet.save(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    }
    // Counts of pixels in one image, and sums of bytes over them: well
    // inside what an f64 holds exactly.
    let f = |n: u64| f64::from(u32::try_from(n).unwrap_or(u32::MAX));
    let (share, mean) = (
        100.0 * f(differing) / f(total.max(1)),
        f(sum) / f(total.max(1)),
    );
    println!(
        "{share:.3}% of pixels differ ({differing} pixels) (mean {mean:.2}/255) over {}x{} — {out}",
        x1 - x0,
        y1 - y0
    );
}
