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
//! `SIGNAL_KIT_STATES=<dir>` also writes, for Studio, one Signal state per
//! drum track (per-track mode, one session: `SIGNAL_KIT_SESSION`, by
//! default `unbreakable-drums`) and a `manifest.json` (see
//! [`write_states`]).
//!
//! `SIGNAL_KIT_CALIBRATE=3` first makes every phase gain neutral on the
//! take (three measure-and-trim passes) and prints the make-ups.
//!
//! `solo` (`Kick/SUM/Sample,Kick/SUM/In`) hears only those tracks (the buses and
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

/// Studio's track for each of [`TRACKS`], in order: what a gain in
/// `SIGNAL_KIT_INPUT_GAINS` is named by.
const STUDIO_TRACKS: &[&str] = &[
    "Drums/Kick/Sum/In",
    "Drums/Kick/Sum/Out",
    "Drums/Kick/Sum/Trig",
    "Drums/Snare/Sum/Top",
    "Drums/Snare/Sum/Bottom",
    "Drums/Snare/Sum/Trig",
    "Drums/Toms/Tom 1/T1",
    "Drums/Toms/Tom 2/T2",
    "Drums/Toms/Tom 3/T3",
    "Drums/Toms/Tom 4/T4",
    "Drums/Toms/Tom 1/T1 Trig",
    "Drums/Toms/Tom 2/T2 Trig",
    "Drums/Toms/Tom 3/T3 Trig",
    "Drums/Toms/Tom 4/T4 Trig",
    "Drums/Cymbals/OHs",
    "Drums/Cymbals/Hi Hat",
    "Drums/Cymbals/Ride",
    "Drums/Rooms",
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
    /// The level the host plays it at (its clip gain), linear.
    gain: f32,
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
            gain: 1.0,
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
                    buf[frame] = v * self.gain;
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
    let mut preset = rigs::drum_kit(&pieces);
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
    let mut paths = Vec::new();
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for track in TRACKS {
        let n = seen.entry(track).or_insert(0);
        let path = find_stem(Path::new(stems), take, track, *n)
            .ok_or_else(|| format!("no stem for {track} (#{n})"))?;
        *n += 1;
        println!("{track:>8}: {}", path.display());
        paths.push(path);
    }
    let length = paths
        .iter()
        .map(|p| Stem::open(p).map(|s| s.reader.duration() as usize))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .unwrap_or(0);
    let frames = seconds.map_or(length, |s| ((s * 48_000.0) as usize).min(length));

    // `SIGNAL_KIT_CALIBRATE=<passes>`: make every phase gain neutral on
    // this take first (each pass measures what the phases do and sets
    // their make-up), then render.
    let passes: usize = std::env::var("SIGNAL_KIT_CALIBRATE")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    for pass in 1..=passes {
        let (levels, _) = render(&preset, &paths, frames, None)?;
        let worst = levels
            .iter()
            .filter(|l| l.in_db > -150.0)
            .map(|l| (l.neutral_trim_db() - l.trim_db).abs())
            .fold(0.0_f32, f32::max);
        signal_core::graph::apply_neutral_trims(&mut preset, &levels);
        if pass == passes {
            for l in levels.iter().filter(|l| {
                l.in_db > -150.0 && l.neutral_trim_db().abs() > signal_core::graph::MAX_TRIM_DB
            }) {
                println!(
                    "  warning: {} {} changes the level by {:+.0} dB: check it",
                    l.lane,
                    l.phase.slug(),
                    -l.neutral_trim_db()
                );
            }
        }
        println!("calibration pass {pass}: the trims moved {worst:.2} dB at most");
    }
    if passes > 0 {
        println!("phase make-ups (gain neutral):");
        for lane in &preset.lanes {
            for t in &lane.trims {
                println!(
                    "  {:<28} {:<10} {:+6.1} dB",
                    lane.name,
                    t.phase.slug(),
                    t.gain_db
                );
            }
        }
    }
    if let Ok(dir) = std::env::var("SIGNAL_KIT_STATES") {
        write_states(&preset, Path::new(&dir))?;
    }
    let (_, peak) = render(&preset, &paths, frames, Some(out))?;
    println!("peak {:.1} dBFS", 20.0 * peak.max(1e-9).log10());
    Ok(())
}

/// Render `frames` of the stems at `paths` through `preset`, to `out` when
/// given. Returns what its phases did, and the peak.
fn render(
    preset: &signal_core::state::Preset,
    paths: &[PathBuf],
    frames: usize,
    out: Option<&str>,
) -> Result<(Vec<signal_core::graph::PhaseLevel>, f32), String> {
    let mut stems_open = paths
        .iter()
        .map(|p| Stem::open(p))
        .collect::<Result<Vec<_>, _>>()?;
    // `SIGNAL_KIT_INPUT_GAINS=<json>`: `{"<Studio track>": dB}`, each stem
    // at the level the host plays it (a comped session's clip gain), so the
    // level-dependent blocks are calibrated where they will run.
    if let Ok(file) = std::env::var("SIGNAL_KIT_INPUT_GAINS") {
        let gains: std::collections::HashMap<String, Option<f32>> = serde_json::from_str(
            &std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?,
        )
        .map_err(|e| format!("{file}: {e}"))?;
        for (stem, track) in stems_open.iter_mut().zip(STUDIO_TRACKS) {
            if let Some(Some(db)) = gains.get(*track) {
                stem.gain = 10f32.powf(db / 20.0);
            }
        }
    }
    let inputs: usize = stems_open.iter().map(|s| s.channels.max(1)).sum();
    if inputs != usize::from(preset.inputs) {
        return Err(format!(
            "{inputs} stem channels; the kit takes {}",
            preset.inputs
        ));
    }
    let factories: [&dyn BlockFactory; 2] = [&BuiltinFactory, &DrumFactory];
    let opts = BuildOptions {
        sample_rate: 48_000.0,
        max_block: BLOCK,
        port: None,
        factories: &factories,
        host: &NoHost,
    };
    let started = std::time::Instant::now();
    let (graph, report) = graph::build(preset, &opts);
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
    let mut writer = match out {
        Some(out) => Some(hound::WavWriter::create(out, spec).map_err(|e| e.to_string())?),
        None => None,
    };
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
                if let Some(w) = writer.as_mut() {
                    w.write_sample(l).map_err(|e| e.to_string())?;
                    w.write_sample(r).map_err(|e| e.to_string())?;
                }
            }
        }
        done += n;
    }
    if let Some(w) = writer {
        w.finalize().map_err(|e| e.to_string())?;
    }
    let took = started.elapsed();
    if let Some(out) = out {
        println!(
            "wrote {out}: {:.1} s in {took:.1?} ({:.1}x real time)",
            frames as f64 / 48_000.0,
            frames as f64 / 48_000.0 / took.as_secs_f64(),
        );
    }
    let levels = proc
        .graph()
        .map(signal_core::graph::Graph::phase_levels)
        .unwrap_or_default();
    Ok((levels, peak))
}

/// What a track of the kit is, for the host: which Signal it runs.
fn role(lane: &signal_core::state::LaneSpec) -> Option<&'static str> {
    use signal_core::block::BlockKind;
    let has = |kind: &str| {
        lane.blocks
            .iter()
            .any(|b| b.kind == BlockKind::Signal(kind.into()))
    };
    let name = lane.name.as_str();
    Some(if lane.trigger.is_some() {
        "trigger"
    } else if has("drum") {
        "sample"
    } else if name.ends_with("/Sub") || name.ends_with("/Fund") {
        "one-note"
    } else if name.starts_with("Drums/Process/Compress/") {
        "parallel"
    } else if has("reverb") {
        "verb"
    } else if !lane.input.is_empty() {
        "mic"
    } else if !lane.blocks.is_empty() {
        "bus"
    } else {
        // Nothing to run (a Sum, a folder): no Signal on it.
        return None;
    })
}

/// For Studio's `SIGNAL_KIT`: one state per track that runs a Signal (the
/// CLAP's own state stream, `plugin::clap_state`: the kit, the per-track
/// mode, the session, the track's path as its port), and
/// `manifest.json`: `{session, clap, tracks: [{path, role, state, to,
/// sends}]}`. `to` and `sends` are the routing the kit assumes and the
/// host provides (a folder's sum, the sends into a Sub, a verb, Process).
fn write_states(preset: &signal_core::state::Preset, dir: &Path) -> Result<(), String> {
    use signal_core::session::{PortName, SessionId, SessionLink};
    use signal_core::state::{Instance, SignalState};
    let session =
        std::env::var("SIGNAL_KIT_SESSION").unwrap_or_else(|_| "unbreakable-drums".into());
    let clap = std::env::var("HOME")
        .map(|h| format!("{h}/.clap/Signal Dev.clap"))
        .unwrap_or_default();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut tracks = Vec::new();
    for lane in &preset.lanes {
        let (Some(role), Some(port)) = (role(lane), lane.port.as_deref()) else {
            continue;
        };
        if port == "Drums/Kit" {
            continue;
        }
        let state = SignalState {
            instance: Instance::Port(SessionLink {
                session: SessionId(session.clone()),
                port: PortName(port.to_owned()),
            }),
            ..SignalState::new(preset.clone())
        };
        let file = format!("{}.state", port.replace('/', "__").replace(' ', "_"));
        std::fs::write(dir.join(&file), signal_plugin::plugin::clap_state(&state))
            .map_err(|e| e.to_string())?;
        tracks.push(serde_json::json!({
            "path": port,
            "role": role,
            "state": file,
            "muted": lane.mute && lane.trigger.is_none(),
        }));
    }
    // The routing the kit assumes and the host provides: every send, by
    // Studio's track paths (`Drums/Kit`, Signal's own sum of the kit, is
    // the pieces' parent: in Studio, the kit's tracks before Process).
    let routing: Vec<_> = preset
        .lanes
        .iter()
        .flat_map(|l| {
            l.sends.iter().map(move |s| {
                serde_json::json!({
                    "from": if l.name == "Drums/Kit" { "Drums (the kit, before Process)" } else { l.name.as_str() },
                    "to": s.to,
                    "gain_db": s.gain_db,
                    "pre": s.pre,
                })
            })
        })
        .collect();
    let manifest = serde_json::json!({ "session": session, "clap": clap, "tracks": tracks, "routing": routing });
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("manifest.json"), text).map_err(|e| e.to_string())?;
    println!(
        "wrote {} track states and the manifest to {}",
        tracks.len(),
        dir.display()
    );
    Ok(())
}
