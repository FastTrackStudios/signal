//! Render a generated control surface to PNG.
//!
//! ```text
//! render_surface --out surface.png [--theme stage-dark|daylight]
//!                [--markup surface.frame] [--catalog catalog.json] [--set amp/gain=0.8 ...]
//!                [--text verb/program="RANDOM HALL" ...]
//!                [--stream ID=V[,V2…] ...] [--stream-file FILE] [--level -12] [--demo-feeds]
//!                [--scale 2] [--fx gpu|fallback|off] [--time SECONDS]
//!                [--frames N [--fps 30] [--columns 4] [--gif anim.gif] [--frames-dir DIR]]
//!                [--set-at SECONDS:addr=v ...] [--stream-at SECONDS:ID=V[,V2…] ...]
//! ```
//! Builds a surface from the demo engine catalog (or `--markup` +
//! `--catalog`), applies the given parameter values (normalized) and stream
//! frames, and renders one frame.
//!
//! `--stream id=v[,v2…]` feeds one frame to any stream — a level in dBFS
//! per channel (`plate/in=-12,-14`), gain reduction (`la2a/gr=-6`), an LFO
//! scalar (`chorus/lfo=0.8`) — and repeats. `--level DB` feeds every level
//! stream in the catalog (DB on the first channel, DB − 4 on the rest)
//! unless `--stream` names streams explicitly; then it only fills in the
//! level streams none of them named.
//!
//! `--frames N` renders an animation: N frames from `--time` at `--fps`,
//! the motions (springs, resonators, LFOs, reels) stepped deterministically
//! from frame to frame. `--out` gets a contact sheet (`--columns` wide),
//! `--gif` a looping GIF, `--frames-dir` one PNG per frame. `--set-at` and
//! `--stream-at` change a param or a feed at a time — a level burst, then
//! silence: `--stream-at 0.2:spring/in=-6,-8 --stream-at 0.5:spring/in=-90,-90`.
//! Feeds are re-sent every frame, as an engine streams them.
//!
//! `--demo-feeds` feeds every other feed of the catalog (levels, gain
//! reduction, spectra, histories, scopes, envelopes, tuner, scalars) from
//! `feed-memory`'s demo generators at `--time` seconds.

#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("render_surface requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(err) = run() {
        eprintln!("render_surface error: {err}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    use frame::surface::proto::{EntryId, ParamAddr, ParamValue, StreamFrame, TextValue};
    use frame::surface::runtime::Runtime;
    use frame::surface::widgets::{Options, Theme, surface_for_catalog};

    let mut out = None;
    let mut theme = Theme::stage_dark();
    let mut sets = Vec::new();
    // Text params' values (`--text addr=STRING`): a program name, a band's name.
    let mut texts: Vec<TextValue> = Vec::new();
    let mut level = None;
    let mut demo_feeds = false;
    let mut streams: Vec<StreamFrame> = Vec::new();
    let mut scale = 1.0_f64;
    let mut markup: Option<String> = None;
    let mut catalog_path: Option<String> = None;
    let mut fx = frame::render::fx::Mode::Gpu;
    let mut time_s = 0.0_f64;
    let mut frames = 0_usize;
    let mut fps = 30.0_f64;
    let mut columns = 4_usize;
    let mut gif: Option<String> = None;
    let mut frames_dir: Option<String> = None;
    // Timed changes: (seconds, what).
    let mut timeline: Vec<(f64, Change)> = Vec::new();
    let mut args = std::env::args().skip(1);
    let at = |kv: &str, what: &str| -> Result<(f64, String), String> {
        let (t, rest) = kv
            .split_once(':')
            .ok_or_else(|| format!("{what} needs SECONDS:…"))?;
        Ok((
            t.parse::<f64>().map_err(|e| format!("{kv}: {e}"))?,
            rest.to_string(),
        ))
    };
    let set = |kv: &str| -> Result<ParamValue, String> {
        let (k, v) = kv.split_once('=').ok_or("--set needs addr=value")?;
        Ok(ParamValue {
            addr: ParamAddr::new(k),
            value: v.parse::<f64>().map_err(|e| format!("{kv}: {e}"))?,
        })
    };
    let stream = |kv: &str| -> Result<StreamFrame, String> {
        let (k, v) = kv.split_once('=').ok_or("--stream needs id=v[,v2…]")?;
        let values = v
            .split(',')
            .map(|x| x.trim().parse::<f64>().map_err(|e| format!("{kv}: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(StreamFrame {
            id: EntryId::new(k),
            values,
        })
    };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next(),
            "--markup" => markup = args.next(),
            "--catalog" => catalog_path = args.next(),
            "--theme" => {
                theme = match args.next().as_deref() {
                    Some("daylight") => Theme::daylight(),
                    _ => Theme::stage_dark(),
                }
            }
            "--set" => sets.push(set(&args.next().ok_or("--set needs addr=value")?)?),
            "--text" => {
                let kv = args.next().ok_or("--text needs addr=text")?;
                let (k, v) = kv.split_once('=').ok_or("--text needs addr=text")?;
                texts.push(TextValue {
                    addr: ParamAddr::new(k),
                    text: v.to_string(),
                });
            }
            "--stream" => streams.push(stream(&args.next().ok_or("--stream needs id=v[,v2…]")?)?),
            "--stream-file" => {
                let path = args.next().ok_or("--stream-file needs a file")?;
                let text =
                    std::fs::read_to_string(&path).map_err(|e| format!("reading {path}: {e}"))?;
                for line in text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                {
                    streams.push(stream(line)?);
                }
            }
            "--set-at" => {
                let (t, kv) = at(
                    &args.next().ok_or("--set-at needs SECONDS:addr=value")?,
                    "--set-at",
                )?;
                timeline.push((t, Change::Set(set(&kv)?)));
            }
            "--stream-at" => {
                let (t, kv) = at(
                    &args.next().ok_or("--stream-at needs SECONDS:id=v[,v2…]")?,
                    "--stream-at",
                )?;
                timeline.push((t, Change::Stream(stream(&kv)?)));
            }
            "--demo-feeds" => demo_feeds = true,
            "--level" => {
                level = Some(
                    args.next()
                        .ok_or("--level needs dB")?
                        .parse::<f64>()
                        .map_err(|e| e.to_string())?,
                )
            }
            "--fx" => fx = args.next().ok_or("--fx needs gpu|fallback|off")?.parse()?,
            "--time" => {
                time_s = args
                    .next()
                    .ok_or("--time needs seconds")?
                    .parse::<f64>()
                    .map_err(|e| e.to_string())?
            }
            "--scale" => {
                scale = args
                    .next()
                    .ok_or("--scale needs a factor")?
                    .parse::<f64>()
                    .map_err(|e| e.to_string())?
            }
            "--frames" => {
                frames = args
                    .next()
                    .ok_or("--frames needs a count")?
                    .parse::<usize>()
                    .map_err(|e| e.to_string())?
            }
            "--fps" => {
                fps = args
                    .next()
                    .ok_or("--fps needs a rate")?
                    .parse::<f64>()
                    .map_err(|e| e.to_string())?
                    .max(1.0)
            }
            "--columns" => {
                columns = args
                    .next()
                    .ok_or("--columns needs a count")?
                    .parse::<usize>()
                    .map_err(|e| e.to_string())?
                    .max(1)
            }
            "--gif" => gif = args.next(),
            "--frames-dir" => frames_dir = args.next(),
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    let out = out.ok_or("--out <png> is required")?;
    timeline.sort_by(|a, b| a.0.total_cmp(&b.0));

    let catalog = match &catalog_path {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?
        }
        None => frame::surface::memory::demo_catalog(),
    };
    let surface = match &markup {
        Some(path) => {
            // `.frame` (or legacy `.fm`) with its `.frame.assets/` textures and fonts, or a `.frame` bundle.
            let scene = frame::document::io::load_surface(path).map_err(|e| e.to_string())?;
            frame::surface::runtime::Surface::from_scene(scene)
        }
        None => surface_for_catalog(
            &catalog,
            &Options {
                theme: theme.clone(),
                ..Options::default()
            },
        ),
    };
    let mut rt = Runtime::new(surface, catalog);
    rt.apply_values(sets);
    rt.apply_texts(texts);
    for f in &streams {
        if rt.catalog().stream(&f.id).is_none() {
            eprintln!(
                "render_surface: warning: no stream `{}` in the catalog",
                f.id.0
            );
        }
    }
    // The feeds held at each instant: `--level`/`--stream` from the start,
    // `--stream-at` from its time on; re-sent every frame, as an engine
    // streams them.
    let levels = level_frames(rt.catalog(), level, &streams);
    let mut held: Vec<StreamFrame> = levels.into_iter().chain(streams).collect();
    let named: Vec<EntryId> = held
        .iter()
        .map(|f| f.id.clone())
        .chain(timeline.iter().filter_map(|(_, c)| match c {
            Change::Stream(f) => Some(f.id.clone()),
            Change::Set(_) => None,
        }))
        .collect();

    let times: Vec<f64> = if frames == 0 {
        vec![time_s]
    } else {
        (0..frames).map(|k| time_s + k as f64 / fps).collect()
    };
    let mut offscreen = frame::render::offscreen::Offscreen::new()?;
    let mut shots = Vec::with_capacity(times.len());
    let mut next = 0;
    let (mut w, mut h) = (0, 0);
    for &t in &times {
        while let Some((at, change)) = timeline.get(next).filter(|(at, _)| *at <= t + 1e-9) {
            let _ = at;
            match change {
                Change::Set(v) => rt.apply_values([v.clone()]),
                Change::Stream(f) => {
                    held.retain(|h| h.id != f.id);
                    held.push(f.clone());
                }
            }
            next += 1;
        }
        let ms = t * 1000.0;
        if demo_feeds {
            let skip: Vec<&EntryId> = named.iter().collect();
            for f in demo_frames(rt.catalog(), t, &skip) {
                rt.apply_frame(f, ms);
            }
        }
        for f in &held {
            rt.apply_frame(f.clone(), ms);
        }
        rt.set_clock(t);
        let frame = rt.frame(ms);

        let page = frame.pages()[0];
        let root = frame
            .node(page)
            .and_then(|p| p.children.first().copied())
            .ok_or("empty surface")?;
        let size = frame.node(root).map(|n| n.size).ok_or("no root")?;
        (w, h) = (
            (size.width * scale).ceil() as u32,
            (size.height * scale).ceil() as u32,
        );
        let bg = frame
            .node(root)
            .and_then(|n| {
                n.fills.iter().find_map(|p| match &p.kind {
                    frame::document::proto::PaintKind::Solid { color } => Some(*color),
                    _ => None,
                })
            })
            .unwrap_or(theme.background);
        let base = peniko::Color::from_rgba8(
            (bg.r * 255.0) as u8,
            (bg.g * 255.0) as u8,
            (bg.b * 255.0) as u8,
            255,
        );
        let values = control_positions(&rt);
        let inputs = rt.shader_inputs();
        let px = offscreen.render_with_effects(&frame::render::fx::Frame {
            transform: kurbo::Affine::scale(scale),
            base,
            fonts: Some(frame::render::FontBook::bundled()),
            mode: fx,
            time_s: t,
            values: Some(&values),
            inputs: Some(&inputs),
            ..frame::render::fx::Frame::new(&frame, page, w, h)
        })?;
        shots.push(
            image::RgbaImage::from_raw(px.width, px.height, px.rgba).ok_or("bad pixel buffer")?,
        );
    }

    if frames == 0 {
        shots[0]
            .save(&out)
            .map_err(|e| format!("writing {out}: {e}"))?;
    } else {
        save_animation(
            &shots,
            fps,
            columns,
            &out,
            gif.as_deref(),
            frames_dir.as_deref(),
        )?;
    }
    println!(
        "{{\"ok\":true,\"out\":\"{out}\",\"width\":{w},\"height\":{h},\"frames\":{},\"controls\":{},\"motions\":{}}}",
        shots.len(),
        rt.surface().controls.len(),
        rt.motions()
    );
    Ok(())
}

/// A timed change for an animation: a param set, or a feed's new value.
#[cfg(feature = "anyrender")]
enum Change {
    Set(frame::surface::proto::ParamValue),
    Stream(frame::surface::proto::StreamFrame),
}

/// The frames as a contact sheet (`columns` wide, left to right, top to
/// bottom) at `out`, and optionally a looping GIF and one PNG per frame.
#[cfg(feature = "anyrender")]
fn save_animation(
    shots: &[image::RgbaImage],
    fps: f64,
    columns: usize,
    out: &str,
    gif: Option<&str>,
    dir: Option<&str>,
) -> Result<(), String> {
    let (w, h) = shots
        .first()
        .map(|s| (s.width(), s.height()))
        .ok_or("no frames")?;
    let cols = columns.min(shots.len()).max(1);
    let rows = shots.len().div_ceil(cols);
    let gap = 4;
    let mut sheet = image::RgbaImage::from_pixel(
        cols as u32 * (w + gap) - gap,
        rows as u32 * (h + gap) - gap,
        image::Rgba([0, 0, 0, 255]),
    );
    for (i, s) in shots.iter().enumerate() {
        let (c, r) = ((i % cols) as u32, (i / cols) as u32);
        image::imageops::overlay(
            &mut sheet,
            s,
            i64::from(c * (w + gap)),
            i64::from(r * (h + gap)),
        );
    }
    sheet.save(out).map_err(|e| format!("writing {out}: {e}"))?;
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir).map_err(|e| format!("{dir}: {e}"))?;
        for (i, s) in shots.iter().enumerate() {
            let p = format!("{dir}/frame-{i:04}.png");
            s.save(&p).map_err(|e| format!("writing {p}: {e}"))?;
        }
    }
    if let Some(path) = gif {
        let file = std::fs::File::create(path).map_err(|e| format!("{path}: {e}"))?;
        let mut enc =
            image::codecs::gif::GifEncoder::new_with_speed(std::io::BufWriter::new(file), 10);
        enc.set_repeat(image::codecs::gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
        let delay = image::Delay::from_numer_denom_ms(1000, fps.round().max(1.0) as u32);
        for s in shots {
            enc.encode_frame(image::Frame::from_parts(s.clone(), 0, 0, delay))
                .map_err(|e| format!("{path}: {e}"))?;
        }
    }
    Ok(())
}

/// `--level`'s frames: `db` (and `db − 4` on further channels) on every
/// level stream of the catalog that no explicit `--stream` names.
#[cfg(feature = "anyrender")]
fn level_frames(
    catalog: &frame::surface::proto::Catalog,
    level: Option<f64>,
    explicit: &[frame::surface::proto::StreamFrame],
) -> Vec<frame::surface::proto::StreamFrame> {
    let Some(db) = level else { return Vec::new() };
    catalog
        .streams
        .iter()
        .filter(|s| !explicit.iter().any(|f| f.id == s.id))
        .filter_map(|s| match s.kind {
            frame::surface::proto::StreamKind::Level { channels } => {
                Some(frame::surface::proto::StreamFrame {
                    id: s.id.clone(),
                    values: (0..channels.max(1))
                        .map(|c| if c == 0 { db } else { db - 4.0 })
                        .collect(),
                })
            }
            _ => None,
        })
        .collect()
}

/// `--demo-feeds`' frames: every feed of the catalog not in `skip`, from
/// the demo generators at `t` seconds.
#[cfg(feature = "anyrender")]
fn demo_frames(
    catalog: &frame::surface::proto::Catalog,
    t: f64,
    skip: &[&frame::surface::proto::EntryId],
) -> Vec<frame::surface::proto::StreamFrame> {
    frame::feed::memory::frames(catalog.streams.iter().filter(|s| !skip.contains(&&s.id)), t)
}

/// Every control's position, for effect shaders' `value`.
#[cfg(feature = "anyrender")]
fn control_positions(
    rt: &frame::surface::runtime::Runtime,
) -> std::collections::HashMap<frame::document::proto::NodeId, f64> {
    rt.surface()
        .controls
        .iter()
        .map(|(id, c)| (*id, rt.position(*id, c)))
        .collect()
}

#[cfg(all(test, feature = "anyrender"))]
mod tests {
    use frame::surface::proto::{Catalog, EntryId, StreamFrame, StreamInfo, StreamKind};

    fn stream(id: &str, kind: StreamKind) -> StreamInfo {
        StreamInfo {
            id: EntryId::new(id),
            name: id.into(),
            kind,
            rate_hz: 30.0,
        }
    }

    #[test]
    fn level_feeds_every_level_stream_an_explicit_stream_leaves_alone() {
        let catalog = Catalog {
            streams: vec![
                stream("plate/in", StreamKind::Level { channels: 2 }),
                stream("plate/out", StreamKind::Level { channels: 1 }),
                stream("plate/gr", StreamKind::GainReduction),
            ],
            ..Catalog::default()
        };
        let ids = |fs: &[StreamFrame]| {
            fs.iter()
                .map(|f| (f.id.0.clone(), f.values.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&super::level_frames(&catalog, Some(-12.0), &[])),
            [
                ("plate/in".to_string(), vec![-12.0, -16.0]),
                ("plate/out".to_string(), vec![-12.0])
            ]
        );
        let explicit = [StreamFrame {
            id: EntryId::new("plate/in"),
            values: vec![-3.0],
        }];
        assert_eq!(
            ids(&super::level_frames(&catalog, Some(-12.0), &explicit)),
            [("plate/out".to_string(), vec![-12.0])]
        );
        assert!(super::level_frames(&catalog, None, &[]).is_empty());
    }

    #[test]
    fn demo_feeds_fill_every_feed_not_named() {
        let catalog = Catalog {
            streams: frame::feed::memory::demo_feeds(),
            ..Catalog::default()
        };
        let skip = EntryId::new("demo/gr");
        let frames = super::demo_frames(&catalog, 1.5, &[&skip]);
        assert_eq!(frames.len(), catalog.streams.len() - 1);
        assert!(frames.iter().all(|f| f.id != skip));
        for f in &frames {
            assert_eq!(
                f.values.len(),
                catalog.stream(&f.id).unwrap().kind.frame_len(),
                "{}",
                f.id
            );
        }
    }
}
