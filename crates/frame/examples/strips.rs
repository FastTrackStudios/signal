//! Render the Tone rack and its panels at strip widths, as PNG strips.
//!
//! `cargo run -p frame-embed --example strips -- [out_dir] [--compare session.png]`
//!
//! Writes, into `out_dir` (default `examples/embed/`):
//! - `mixer-row.png`: the channel rack at session's widths — a rail (40),
//!   REAPER's strip (86), a working strip (133), a focused one (300) — with
//!   demo settings and feeds.
//! - `mixer-returns.png`: bus, delay and reverb returns at 86 and 133.
//! - `panel-<unit>.png`: each unit alone at MCP widths 60, 96, 160 (x1) and
//!   96 at x2.
//! - `featured.png`: the generic featured-param embed at REAPER's sizes.
//! - `compare.png` (with `--compare`): the mixer row beside a crop of
//!   session's own rendering.

use std::path::PathBuf;
use std::time::Instant;

use frame::embed::cpu::{Pixels, Rasterizer, snapshot};
use frame::embed::tone::{Which, chains, demo};
use frame::embed::{Embed, EmbedHost, FeaturedView, TonePanel, ToneRack};

const BG: [u8; 4] = [0x09, 0x09, 0x0b, 0xff];

fn fed(mut e: Embed, units: &[Which], index: usize) -> Embed {
    let values = demo::values(e.catalog(), units, index);
    e.apply_values(values);
    for f in demo::frames(e.catalog(), units, index, 40) {
        e.apply_frame(f, 1000.0);
    }
    e
}

fn rack(units: &[Which], width: f64, index: usize) -> Embed {
    let mut e = Embed::new(ToneRack::new(units), EmbedHost::Strip);
    let e0 = {
        e.set_size(width, 100.0);
        e.height_for(width).unwrap_or(600.0)
    };
    e.set_size(width, e0);
    fed(e, units, index)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut out = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/embed"));
    let mut compare: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        if a == "--compare" {
            compare = args.next().map(PathBuf::from);
        } else {
            out = PathBuf::from(a);
        }
    }
    std::fs::create_dir_all(&out).expect("out dir");
    let mut raster = Rasterizer::new();

    // The mixer row.
    let widths = [40.0, 86.0, 133.0, 300.0];
    let t0 = Instant::now();
    let row: Vec<Pixels> = widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            snapshot(
                &mut raster,
                &mut rack(&chains::CHANNEL, *w, i + 1),
                1.0,
                1000.0,
            )
        })
        .collect();
    let mixer = Pixels::row(&row, 2, BG);
    mixer.save_png(out.join("mixer-row.png")).expect("png");
    eprintln!(
        "mixer-row: {}x{} in {:?}",
        mixer.width,
        mixer.height,
        t0.elapsed()
    );

    // The same row with Rescue unfolded: every unit of the chain.
    let open: Vec<Pixels> = widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let mut e = rack(&chains::CHANNEL, *w, i + 1);
            e.apply_values([frame::surface::proto::ParamValue {
                addr: frame::surface::proto::ParamAddr::new("rack/fold_rescue"),
                value: 0.0,
            }]);
            let h = e.height_for(*w).unwrap_or(1800.0);
            e.set_size(*w, h);
            snapshot(&mut raster, &mut e, 1.0, 1000.0)
        })
        .collect();
    Pixels::row(&open, 2, BG)
        .save_png(out.join("mixer-row-open.png"))
        .expect("png");

    let mut returns = Vec::new();
    for w in [86.0, 133.0] {
        for (i, units) in [
            &chains::BUS[..],
            &chains::DELAY[..],
            &chains::REVERB[..],
            &chains::FUND[..],
        ]
        .into_iter()
        .enumerate()
        {
            returns.push(snapshot(
                &mut raster,
                &mut rack(units, w, 10 + i),
                1.0,
                1000.0,
            ));
        }
    }
    Pixels::row(&returns, 2, BG)
        .save_png(out.join("mixer-returns.png"))
        .expect("png");

    // Each unit alone at MCP widths.
    for which in Which::ALL {
        if matches!(
            which,
            Which::RescueEq | Which::RescueComp | Which::PolishEq | Which::Space
        ) {
            continue;
        }
        let mut shots = Vec::new();
        for (w, scale) in [
            (60.0, 1.0),
            (96.0, 1.0),
            (160.0, 1.0),
            (300.0, 1.0),
            (96.0, 2.0),
        ] {
            let mut e = Embed::new(TonePanel::new(which), EmbedHost::Mcp);
            e.set_size(w, which.natural());
            let mut e = fed(e, &[which], 3);
            shots.push(snapshot(&mut raster, &mut e, scale, 1000.0));
        }
        let name = which.ns();
        Pixels::row(&shots, 4, BG)
            .save_png(out.join(format!("panel-{name}.png")))
            .expect("png");
    }

    // The generic embed at REAPER's sizes.
    let catalog = frame::surface::memory::demo_catalog();
    let mut shots = Vec::new();
    for (w, h) in [(60.0, 120.0), (96.0, 200.0), (160.0, 300.0), (300.0, 40.0)] {
        let host = if h < w {
            EmbedHost::Tcp
        } else {
            EmbedHost::Mcp
        };
        let mut e = Embed::new(FeaturedView::new(catalog.clone()), host);
        e.set_size(w, h);
        shots.push(snapshot(&mut raster, &mut e, 1.0, 0.0));
    }
    Pixels::row(&shots, 4, BG)
        .save_png(out.join("featured.png"))
        .expect("png");

    if let Some(path) = compare {
        let img = image::open(&path).expect("session png").to_rgba8();
        // session's golden scenes are 2560 x 1440; the first strips' racks
        // start about 44 px in and 30 px down.
        let (cw, ch) = (
            row.iter()
                .map(|p| p.width)
                .sum::<u32>()
                .min(img.width() - 40),
            mixer.height.min(img.height() - 30).min(900),
        );
        let crop = image::imageops::crop_imm(&img, 44, 30, cw, ch).to_image();
        let theirs = Pixels {
            width: cw,
            height: ch,
            rgba: crop.into_raw(),
        };
        let ours = Pixels::column(std::slice::from_ref(&mixer), 0, BG);
        let ours = Pixels {
            width: ours.width,
            height: ch.min(ours.height),
            rgba: ours.rgba[..(ours.width * ch.min(ours.height) * 4) as usize].to_vec(),
        };
        Pixels::row(&[theirs, ours], 12, [0x30, 0x30, 0x36, 0xff])
            .save_png(out.join("compare.png"))
            .expect("png");
    }
    eprintln!("wrote {}", out.display());
}
