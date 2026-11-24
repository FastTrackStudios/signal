//! Render a multitrack drum take through Signal's drum kit
//! (`signal_core::rigs::drum_kit`): the mics and triggers in, library
//! reinforcement on the triggers, the one-note chains, the buses and the
//! parallel compression, a stereo mix out.
//!
//! ```text
//! cargo run -p signal-plugin --release --example render_kit -- \
//!     <stems dir> <take suffix> <library dir> <out.wav> [seconds] [solo]
//! ```
//!
//! `solo` (`Kick Sample,Kick In`) hears only those tracks (the buses and
//! the triggers still run): to check one against another.
//!
//! The stems are named as a REAPER session names them, `<n>-<track>-<take
//! suffix>.wav` (`65-In-260907_1321-01.wav`): the kit's tracks are found
//! by their name (`In`, `Out`, `Trig` the first time, the snare's
//! afterwards …), in the kit's order. The library is a Signal drum library
//! (Modern and Massive 2's `Engines/`). Nothing here is committed: stems
//! and libraries are the user's.

use std::path::{Path, PathBuf};

use signal_core::block::BlockFactory;
use signal_core::graph::{self, BuildOptions};
use signal_core::host::{AssetRef, NoHost};
use signal_core::rigs::{self, KitPieces};
use signal_core::{BuiltinFactory, Processor};
use signal_plugin::drum::DrumFactory;

const BLOCK: usize = 512;

/// The kit's inputs, in its order, by the session's track names: kick
/// (In, Out, Trig), snare (Top, Bottom, Trig), T1–T4, their triggers, OHs,
/// Hi Hat, Ride, Rooms.
const TRACKS: &[&str] = &[
    "In", "Out", "Trig", "Top", "Bottom", "Trig", "T1", "T2", "T3", "T4", "T1 Trig", "T2 Trig",
    "T3 Trig", "T4 Trig", "OHs", "Hi Hat", "Ride", "Rooms",
];

/// The library's pieces this kit uses: a matched Tama Starclassic Bubinga
/// kit, and a Tama snare.
const KICK: &str = "MM2 22x18-Tama-Starclassic-Bubinga-Kick";
const SNARE: &str = "MM2 14x6-Tama-Abe-Cunningham-Signature-Snare";
const TOMS: [&str; 4] = [
    "MM2 12x10-Tama-Starclassic-Bubinga-Tom",
    "MM2 14x11-Tama-Starclassic-Bubinga-Tom",
    "MM2 16x16-Tama-Starclassic-Bubinga-Tom",
    "MM2 18x16-Tama-Starclassic-Bubinga-Tom",
];

struct Stem {
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    channels: usize,
    scale: f32,
}

impl Stem {
    fn open(path: &Path) -> Result<Self, String> {
        let reader =
            hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let spec = reader.spec();
        if spec.sample_rate != 48_000 {
            return Err(format!(
                "{}: {} Hz; the kit renders at 48 kHz",
                path.display(),
                spec.sample_rate
            ));
        }
        let scale = match spec.sample_format {
            hound::SampleFormat::Float => 1.0,
            hound::SampleFormat::Int => 1.0 / (1_u64 << (spec.bits_per_sample - 1)) as f32,
        };
        Ok(Self {
            channels: usize::from(spec.channels),
            scale,
            reader,
        })
    }

    /// The next `frames` into `out` (one buffer per channel; a mono stem
    /// fills the first). Zeros past the end.
    fn read(&mut self, frames: usize, out: &mut [&mut [f32]]) {
        let float = self.reader.spec().sample_format == hound::SampleFormat::Float;
        for frame in 0..frames {
            for c in 0..self.channels {
                let v = if float {
                    self.reader
                        .samples::<f32>()
                        .next()
                        .and_then(Result::ok)
                        .unwrap_or(0.0)
                } else {
                    self.reader
                        .samples::<i32>()
                        .next()
                        .and_then(Result::ok)
                        .map_or(0.0, |s| s as f32 * self.scale)
                };
                if let Some(buf) = out.get_mut(c) {
                    buf[frame] = v;
                }
            }
        }
    }
}

/// The stem of `track` (its `occurrence`-th match) in `dir`.
fn find_stem(dir: &Path, take: &str, track: &str, occurrence: usize) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let Some(rest) = name.strip_suffix(&format!("-{take}.wav")) else {
                return false;
            };
            // `<n>-<track>`
            rest.split_once('-')
                .is_some_and(|(n, t)| n.chars().all(|c| c.is_ascii_digit()) && t == track)
        })
        .collect();
    found.sort();
    found.into_iter().nth(occurrence)
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let [_, stems, take, library, out, rest @ ..] = args.as_slice() else {
        return Err(
            "usage: render_kit <stems dir> <take suffix> <library dir> <out.wav> [seconds] [solo]"
                .into(),
        );
    };
    let seconds: Option<f64> = rest.first().and_then(|s| s.parse().ok());
    let engines = Path::new(library).join("Engines");
    let piece = |name: &str| {
        let p = engines.join(format!("{name}.signalengine"));
        p.exists().then(|| AssetRef::path(p.display().to_string()))
    };
    let pieces = KitPieces {
        kick: piece(KICK),
        snare: piece(SNARE),
        toms: TOMS.map(piece),
    };
    // `SIGNAL_KIT_STYLE=modern`: the modern mix.
    let mut preset = match std::env::var("SIGNAL_KIT_STYLE").as_deref() {
        Ok("modern") => rigs::drum_kit_modern(&pieces),
        _ => rigs::drum_kit(&pieces),
    };
    // `SIGNAL_KIT_STRIP=Kick,Drums`: those lanes without their blocks (to
    // find which stage does what).
    if let Ok(strip) = std::env::var("SIGNAL_KIT_STRIP") {
        let strip: Vec<&str> = strip.split(',').map(str::trim).collect();
        for lane in &mut preset.lanes {
            if strip.contains(&lane.name.as_str()) {
                lane.blocks
                    .retain(|b| b.kind == signal_core::block::BlockKind::Signal("drum".into()));
            }
        }
    }
    if let Some(solo) = rest.get(1) {
        let solo: Vec<&str> = solo.split(',').map(str::trim).collect();
        // The sources not soloed are silenced: the tracks with an input, and
        // the ones a trigger plays. Buses, verbs and the parallels stay.
        for lane in &mut preset.lanes {
            let source = !lane.input.is_empty() || lane.blocks.iter().any(|b| b.key.is_some());
            if source && lane.trigger.is_none() && !solo.contains(&lane.name.as_str()) {
                // Down, not muted: its pre sends (into a one-note chain)
                // still go, as a DAW's pre-fader sends do.
                lane.gain_db = -240.0;
            }
        }
    }

    // The stems, in the kit's input order: one channel each, OHs and
    // Rooms two.
    let mut stems_open = Vec::new();
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for track in TRACKS {
        let n = seen.entry(track).or_insert(0);
        let path = find_stem(Path::new(stems), take, track, *n)
            .ok_or_else(|| format!("no stem for {track} (#{n})"))?;
        *n += 1;
        println!("{track:>8}: {}", path.display());
        stems_open.push(Stem::open(&path)?);
    }
    let inputs: usize = stems_open.iter().map(|s| s.channels.max(1)).sum();
    if inputs != usize::from(preset.inputs) {
        return Err(format!(
            "{inputs} stem channels; the kit takes {}",
            preset.inputs
        ));
    }
    let length = stems_open
        .iter()
        .map(|s| s.reader.duration() as usize)
        .max()
        .unwrap_or(0);
    let frames = seconds.map_or(length, |s| ((s * 48_000.0) as usize).min(length));

    let factories: [&dyn BlockFactory; 2] = [&BuiltinFactory, &DrumFactory];
    let opts = BuildOptions {
        sample_rate: 48_000.0,
        max_block: BLOCK,
        port: None,
        factories: &factories,
        host: &NoHost,
    };
    let started = std::time::Instant::now();
    let (graph, report) = graph::build(&preset, &opts);
    for b in &report.blocks {
        if let Some(why) = b.status.reason() {
            println!("  {}/{}: {why}", b.lane, b.block);
        }
    }
    for w in &report.warnings {
        println!("  warning: {w}");
    }
    let latency = graph.latency() as usize;
    println!(
        "kit built in {:.1?}: {} blocks, latency {latency} frames; rendering {:.1} s",
        started.elapsed(),
        report.blocks.len(),
        frames as f64 / 48_000.0
    );
    let mut proc = Processor::new();
    let _ = proc.install(graph);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(out, spec).map_err(|e| e.to_string())?;
    let mut bufs = vec![vec![0.0_f32; BLOCK]; inputs];
    let started = std::time::Instant::now();
    let mut peak = 0.0_f32;
    // The kit's latency in, then dropped from the front: the mix lines up
    // with the stems.
    let mut done = 0;
    while done < frames + latency {
        let n = BLOCK.min(frames + latency - done);
        let mut c = 0;
        for stem in &mut stems_open {
            let width = stem.channels.max(1);
            let mut chans: Vec<&mut [f32]> =
                bufs[c..c + width].iter_mut().map(|b| &mut b[..n]).collect();
            stem.read(n, &mut chans);
            c += width;
        }
        let mut io: Vec<&mut [f32]> = bufs.iter_mut().map(|b| &mut b[..n]).collect();
        proc.process(&mut io, inputs, n, &[], &[]);
        for i in 0..n {
            if done + i >= latency {
                let (l, r) = (bufs[0][i], bufs[1][i]);
                peak = peak.max(l.abs()).max(r.abs());
                writer.write_sample(l).map_err(|e| e.to_string())?;
                writer.write_sample(r).map_err(|e| e.to_string())?;
            }
        }
        done += n;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    let took = started.elapsed();
    println!(
        "wrote {out}: {:.1} s in {took:.1?} ({:.1}x real time), peak {:.1} dBFS",
        frames as f64 / 48_000.0,
        frames as f64 / 48_000.0 / took.as_secs_f64(),
        20.0 * peak.max(1e-9).log10()
    );
    Ok(())
}
