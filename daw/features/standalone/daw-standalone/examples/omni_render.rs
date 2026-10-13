//! Reference renderer: play an Omnisphere patch (`.prt_omn`) through the
//! real Omnisphere VST3 and write the result to a 32-bit float WAV.
//!
//! ```text
//! cargo +1.94.0 run --release -p daw-standalone --features vst3-host \
//!     --example omni_render -- <patch.prt_omn | default> <out.wav> \
//!     [--note 48 --vel 100 --hold 2.0 --tail 2.0 --sr 48000 \
//!      --part 1 --preroll 1.0 --dump-state <file> --plugin <bundle.vst3>]
//! ```
//!
//! How the patch gets in: Omnisphere's VST3 component state is plain
//! XML wrapped in a small binary envelope:
//!
//! ```text
//! u32 LE 999999999 (0x3B9AC9FF) | u32 0 | u32 1 | u32 0   — header
//! u64 LE N                                                — XML length
//! N bytes  <SynthMaster vers=..> ... </SynthMaster>       — the multi
//! trailer  (zero padding + JUCE's "JUCEPrivateData" footer)
//! ```
//!
//! The multi holds eight `<SynthSubEngine>` parts, each wrapping one
//! `<SynthEngine>` element. A `.prt_omn` file is
//! `<AmberPart><SynthEngine>..</SynthEngine></AmberPart>` — the same
//! element — so we splice the patch's `<SynthEngine>` over the chosen
//! part's, fix the length field, keep the trailer verbatim, and
//! `setState` it back. The re-read state is checked for the patch
//! name to confirm Omnisphere accepted it.
//!
//! Pass `default` instead of a patch path to render the init multi.

use std::ffi::c_void;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use daw_proto::{Channel, ControllerNumber, ControllerValue, KeyNumber, MidiEvent, Velocity};
use daw_standalone::audio_engine::vst3_host::{LoadedVst3Plugin, Vst3Host};
use daw_standalone::plugin::{PluginEvents, PluginMidiEvent};

const OMNI_MAGIC: u32 = 999_999_999;
const HEADER_LEN: usize = 24;
const BLOCK: usize = 512;

struct Args {
    patch: Option<PathBuf>,
    out: PathBuf,
    note: u8,
    vel: u8,
    hold: f64,
    tail: f64,
    sr: u32,
    part: usize,
    preroll: f64,
    dump_state: Option<PathBuf>,
    plugin: PathBuf,
    /// Controller values `(cc, value)` sent before the note (mod wheel = 1).
    cc: Vec<(u8, u8)>,
    /// A note held through the pre-roll and released just after the main
    /// note starts (legato), for glide measurements.
    prev: Option<u8>,
    /// Host tempo (the process context's).
    bpm: f64,
}

fn usage() -> ! {
    eprintln!(
        "usage: omni_render <patch.prt_omn|default> <out.wav> [--note 48] [--vel 100] \
         [--hold 2.0] [--tail 2.0] [--sr 48000] [--part 1] [--preroll 1.0] \
         [--dump-state FILE] [--plugin BUNDLE.vst3] [--cc N=V ...] [--prev NOTE] [--bpm 120]"
    );
    std::process::exit(2)
}

fn parse_args() -> Args {
    let mut pos = Vec::new();
    let mut a = Args {
        patch: None,
        out: PathBuf::new(),
        note: 48,
        vel: 100,
        hold: 2.0,
        tail: 2.0,
        sr: 48_000,
        part: 1,
        preroll: 1.0,
        dump_state: None,
        plugin: PathBuf::from("/Library/Audio/Plug-Ins/VST3/Omnisphere.vst3"),
        cc: Vec::new(),
        prev: None,
        bpm: 120.0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| usage());
        match arg.as_str() {
            "--note" => a.note = val().parse().unwrap_or_else(|_| usage()),
            "--vel" => a.vel = val().parse().unwrap_or_else(|_| usage()),
            "--hold" => a.hold = val().parse().unwrap_or_else(|_| usage()),
            "--tail" => a.tail = val().parse().unwrap_or_else(|_| usage()),
            "--sr" => a.sr = val().parse().unwrap_or_else(|_| usage()),
            "--part" => a.part = val().parse().unwrap_or_else(|_| usage()),
            "--preroll" => a.preroll = val().parse().unwrap_or_else(|_| usage()),
            "--dump-state" => a.dump_state = Some(PathBuf::from(val())),
            "--plugin" => a.plugin = PathBuf::from(val()),
            "--prev" => a.prev = Some(val().parse().unwrap_or_else(|_| usage())),
            "--bpm" => a.bpm = val().parse().unwrap_or_else(|_| usage()),
            "--cc" => {
                let v = val();
                let (n, x) = v.split_once('=').unwrap_or_else(|| usage());
                a.cc.push((
                    n.parse().unwrap_or_else(|_| usage()),
                    x.parse().unwrap_or_else(|_| usage()),
                ));
            }
            "-h" | "--help" => usage(),
            s if s.starts_with("--") => usage(),
            _ => pos.push(arg),
        }
    }
    if pos.len() != 2 || !(1..=8).contains(&a.part) || a.note > 127 || a.vel > 127 {
        usage();
    }
    a.patch = (pos[0] != "default").then(|| PathBuf::from(&pos[0]));
    a.out = PathBuf::from(&pos[1]);
    a
}

// ── Omnisphere state envelope ────────────────────────────────────────

struct OmniState {
    header: Vec<u8>,
    xml: String,
    trailer: Vec<u8>,
}

impl OmniState {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < HEADER_LEN {
            return Err("state shorter than header".into());
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != OMNI_MAGIC {
            return Err(format!("unexpected state magic {magic:#x}"));
        }
        let n = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
        let end = HEADER_LEN + n;
        if end > bytes.len() {
            return Err(format!(
                "xml length {n} overruns state ({} bytes)",
                bytes.len()
            ));
        }
        let xml = std::str::from_utf8(&bytes[HEADER_LEN..end])
            .map_err(|e| format!("state xml not utf-8: {e}"))?
            .to_owned();
        Ok(Self {
            header: bytes[..16].to_vec(),
            xml,
            trailer: bytes[end..].to_vec(),
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.xml.len() + self.trailer.len());
        out.extend_from_slice(&self.header);
        out.extend_from_slice(&(self.xml.len() as u64).to_le_bytes());
        out.extend_from_slice(self.xml.as_bytes());
        out.extend_from_slice(&self.trailer);
        out
    }

    /// Byte range of the `index`-th (0-based) `<SynthSubEngine>`'s
    /// `<SynthEngine>..</SynthEngine>` element.
    fn part_engine_range(&self, index: usize) -> Result<(usize, usize), String> {
        let x = &self.xml;
        let mut from = 0;
        for _ in 0..index {
            from += x[from..].find("<SynthSubEngine>").ok_or("missing part")? + 1;
        }
        let sub = from + x[from..].find("<SynthSubEngine>").ok_or("missing part")?;
        element_range(x, sub, "SynthEngine").ok_or_else(|| format!("part {index}: no SynthEngine"))
    }

    fn part_name(&self, index: usize) -> Option<String> {
        let (s, e) = self.part_engine_range(index).ok()?;
        entry_name(&self.xml[s..e])
    }
}

/// Find `<tag>..</tag>` starting at or after `from` (tags don't nest
/// for SynthEngine) and return its byte range.
fn element_range(x: &str, from: usize, tag: &str) -> Option<(usize, usize)> {
    // `<Tag>` or `<Tag …>` (older patches write `<SynthEngine >`).
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut at = from;
    let s = loop {
        let i = at + x[at..].find(&open)?;
        match x[i + open.len()..].chars().next() {
            Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r') => break i,
            _ => at = i + open.len(),
        }
    };
    let e = s + x[s..].find(&close)? + close.len();
    Some((s, e))
}

fn entry_name(engine_xml: &str) -> Option<String> {
    let i = engine_xml.find("<ENTRYDESCR")?;
    let rest = &engine_xml[i..];
    let n = rest.find("name=\"")? + 6;
    let len = rest[n..].find('"')?;
    Some(rest[n..n + len].to_owned())
}

// ── Plugin driving ───────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn pump_run_loop(secs: f64) {
    // Omnisphere is a JUCE plugin: patch/sample loading finishes via
    // messages posted to the main run loop. Nothing runs it in a CLI
    // host, so pump it by hand.
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, ret: u8) -> i32;
    }
    let deadline = Instant::now() + Duration::from_secs_f64(secs);
    loop {
        // SAFETY: plain CoreFoundation call on the main thread.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.01, 0);
        }
        if Instant::now() >= deadline {
            break;
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn pump_run_loop(secs: f64) {
    std::thread::sleep(Duration::from_secs_f64(secs));
}

fn component_state(p: &mut LoadedVst3Plugin) -> Vec<u8> {
    let blob = p.save_state().expect("getState failed");
    let n = u32::from_le_bytes(blob[4..8].try_into().unwrap()) as usize;
    blob[8..8 + n].to_vec()
}

fn set_component_state(p: &mut LoadedVst3Plugin, comp: &[u8]) {
    // Wrap in the host's DAW3 blob (component state, empty controller
    // state) and hand it to load_state → IComponent::setState.
    let mut blob = Vec::with_capacity(comp.len() + 12);
    blob.extend_from_slice(b"DAW3");
    blob.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    blob.extend_from_slice(comp);
    blob.extend_from_slice(&0u32.to_le_bytes());
    p.load_state(&blob).expect("setState failed");
}

fn render(
    p: &mut LoadedVst3Plugin,
    frames: usize,
    mut events_at: impl FnMut(usize) -> Vec<PluginMidiEvent>,
) -> (Vec<f32>, Vec<f32>) {
    let zeros = vec![0.0f32; BLOCK];
    let mut l = vec![0.0f32; frames];
    let mut r = vec![0.0f32; frames];
    // OMNI_PARAMS="9=0.264,6=0.264": host parameters (plain values) held
    // every block — e.g. a gig's widgets driving Omnisphere's MIDI-learned
    // automation slots.
    let params: Vec<(u32, f64)> = std::env::var("OMNI_PARAMS")
        .ok()
        .map(|v| {
            v.split(',')
                .filter_map(|kv| {
                    let (k, v) = kv.split_once('=')?;
                    Some((k.trim().parse().ok()?, v.trim().parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        let midi = events_at(pos);
        let ev = PluginEvents {
            params: &params,
            midi: &midi,
            note_expressions: &[],
        };
        p.process_block(
            &zeros[..n],
            &zeros[..n],
            &mut l[pos..pos + n],
            &mut r[pos..pos + n],
            &ev,
        )
        .expect("process failed");
        pos += n;
    }
    (l, r)
}

// ── Output + analysis ────────────────────────────────────────────────

fn write_wav(path: &PathBuf, sr: u32, l: &[f32], r: &[f32]) -> std::io::Result<()> {
    let data_len = (l.len() * 2 * 4) as u32;
    let mut b = Vec::with_capacity(data_len as usize + 44);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&sr.to_le_bytes());
    b.extend_from_slice(&(sr * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for (a, c) in l.iter().zip(r) {
        b.extend_from_slice(&a.to_le_bytes());
        b.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(path, b)
}

fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len() as f64).sqrt()
}

/// Power-weighted mean spectral centroid (Hz) over Hann windows of the
/// mono signal. Naive DFT — the renders are short.
fn spectral_centroid(mono: &[f32], sr: u32) -> f64 {
    const N: usize = 2048;
    let win: Vec<f64> = (0..N)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / N as f64).cos())
        .collect();
    let (cos, sin): (Vec<f64>, Vec<f64>) = (0..N)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / N as f64;
            (a.cos(), a.sin())
        })
        .unzip();
    let (mut num, mut den) = (0.0, 0.0);
    let hop = (mono.len() / 12).max(N);
    let mut start = 0;
    while start + N <= mono.len() {
        let frame: Vec<f64> = (0..N).map(|i| mono[start + i] as f64 * win[i]).collect();
        for k in 1..N / 2 {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &s) in frame.iter().enumerate() {
                let idx = (i * k) % N;
                re += s * cos[idx];
                im -= s * sin[idx];
            }
            let p = re * re + im * im;
            num += p * (k as f64 * sr as f64 / N as f64);
            den += p;
        }
        start += hop;
    }
    if den > 0.0 { num / den } else { 0.0 }
}

fn main() {
    let a = parse_args();
    let sr = a.sr as f64;
    let t0 = Instant::now();

    let host = Vst3Host::new();
    let mut plugin = host
        .load(&a.plugin, 0)
        .expect("failed to load plugin bundle");
    eprintln!(
        "[{:>6.2?}] loaded {}",
        t0.elapsed(),
        plugin.descriptor().name
    );

    if std::env::var_os("OMNI_LIST_PARAMS").is_some() {
        for p in plugin.params() {
            println!("{}\t{}", p.id, p.name);
        }
        return;
    }

    // Default multi as the plugin comes up.
    let base = OmniState::parse(&component_state(&mut plugin)).expect("parse default state");
    let part = a.part - 1;
    eprintln!(
        "[{:>6.2?}] default state: {} bytes xml, part {} = {:?}",
        t0.elapsed(),
        base.xml.len(),
        a.part,
        base.part_name(part)
    );

    let mut expect_name = None;
    // OMNI_STATE=<file>: a whole captured multi (the component state as
    // Omnisphere writes it — e.g. a gig's instance, decoded), instead of a
    // patch spliced into the default one. Play its parts by `--part`.
    if let Some(path) = std::env::var_os("OMNI_STATE") {
        let bytes = std::fs::read(&path).expect("read OMNI_STATE");
        set_component_state(&mut plugin, &bytes);
        pump_run_loop(1.0);
        eprintln!("[{:>6.2?}] setState from {}", t0.elapsed(), std::path::Path::new(&path).display());
    } else if a.patch.is_none() {
        // Round-trip the untouched default multi through setState so
        // both renders take the same path through the plugin.
        set_component_state(&mut plugin, &base.to_bytes());
        pump_run_loop(0.5);
    }
    if let Some(patch) = &a.patch {
        let text = std::fs::read_to_string(patch).expect("read patch");
        let (ps, pe) = element_range(&text, 0, "SynthEngine").expect("patch has no <SynthEngine>");
        // The plugin writes its state with inter-element whitespace
        // collapsed to spaces; match that.
        let engine = text[ps..pe].replace("\r\n", " ").replace('\n', " ");
        expect_name = entry_name(&engine);
        let (s, e) = base.part_engine_range(part).expect("locate part");
        let mut st = OmniState {
            header: base.header.clone(),
            xml: base.xml.clone(),
            trailer: base.trailer.clone(),
        };
        st.xml.replace_range(s..e, &engine);
        // OMNI_PLEVEL=0.44: the part's mixer fader (`pLevelN`, 0.75 at
        // init), for measuring its law.
        if let Some(level) = std::env::var("OMNI_PLEVEL").ok().and_then(|v| v.parse::<f32>().ok()) {
            let key = format!("pLevel{part}=\"");
            let at = st.xml.find(&key).expect("multi has no part level") + key.len();
            let end = at + st.xml[at..].find('"').expect("unterminated pLevel");
            st.xml.replace_range(at..end, &format!("{:08x}", level.to_bits()));
        }
        set_component_state(&mut plugin, &st.to_bytes());
        pump_run_loop(0.5);
        eprintln!(
            "[{:>6.2?}] setState with patch {:?}",
            t0.elapsed(),
            expect_name
        );
    }

    plugin.prepare(sr, BLOCK as u32).expect("prepare");
    plugin.set_tempo(a.bpm);

    // Pre-roll: let the engine (and any sample streaming) settle,
    // pumping the run loop between blocks.
    let pre_frames = (a.preroll * sr) as usize;
    let ccs: Vec<PluginMidiEvent> = a
        .cc
        .iter()
        .map(|&(n, v)| PluginMidiEvent {
            offset: 0,
            message: MidiEvent::ControlChange {
                channel: Channel::new((a.part - 1) as u8),
                controller: ControllerNumber::new(n),
                value: ControllerValue::new(v),
            },
        })
        .collect();
    let prev_ch = Channel::new((a.part - 1) as u8);
    let mut done = 0;
    while done < pre_frames {
        let n = BLOCK.min(pre_frames - done);
        let first = done == 0;
        // The previous note starts half a second before the main one.
        let prev_on = a.prev.filter(|_| {
            let at = pre_frames.saturating_sub((0.5 * sr) as usize);
            (done..done + n).contains(&at)
        });
        render(&mut plugin, n, |_| {
            let mut ev = if first { ccs.clone() } else { Vec::new() };
            if let Some(k) = prev_on {
                ev.push(PluginMidiEvent {
                    offset: 0,
                    message: MidiEvent::NoteOn {
                        channel: prev_ch,
                        key: KeyNumber::new(k),
                        velocity: Velocity::new(a.vel),
                    },
                });
            }
            ev
        });
        pump_run_loop(0.0);
        done += n;
    }

    // Confirm what's loaded.
    let now = OmniState::parse(&component_state(&mut plugin)).expect("parse state");
    let loaded = now.part_name(part);
    eprintln!(
        "[{:>6.2?}] part {} now = {:?}",
        t0.elapsed(),
        a.part,
        loaded
    );
    if let Some(p) = &a.dump_state {
        std::fs::write(p, now.xml.as_bytes()).expect("dump state");
    }
    if expect_name.is_some() && loaded != expect_name {
        eprintln!("WARNING: patch name not reflected in plugin state — patch may not have loaded");
    }

    let hold_frames = (a.hold * sr) as usize;
    let total = hold_frames + (a.tail * sr) as usize;
    let key = KeyNumber::new(a.note);
    let ch = Channel::new((a.part - 1) as u8);
    let vel = a.vel;
    let chord: Vec<u8> = std::env::var("OMNI_CHORD")
        .ok()
        .map(|v| v.split(',').filter_map(|k| k.trim().parse().ok()).collect())
        .unwrap_or_default();
    let (l, r) = render(&mut plugin, total, |pos| {
        let mut ev = Vec::new();
        if pos == 0 {
            ev.push(PluginMidiEvent {
                offset: 0,
                message: MidiEvent::NoteOn {
                    channel: ch,
                    key,
                    velocity: Velocity::new(vel),
                },
            });
        }
        if pos == 0 {
            if let Some(k) = a.prev {
                ev.push(PluginMidiEvent {
                    offset: 1,
                    message: MidiEvent::NoteOff {
                        channel: ch,
                        key: KeyNumber::new(k),
                        velocity: Velocity::new(0),
                    },
                });
            }
        }
        // OMNI_CHORD="52,55,60": extra notes held with the main one, each
        // 50 ms after the last (a voice limit then steals the earliest).
        let stagger = std::env::var("OMNI_STAGGER_MS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map_or(sr as usize / 20, |ms| (ms / 1000.0 * sr) as usize);
        for (i, k) in chord.iter().enumerate() {
            let at = (i + 1) * stagger;
            if (pos..pos + BLOCK).contains(&at) {
                ev.push(PluginMidiEvent {
                    offset: (at - pos) as u32,
                    message: MidiEvent::NoteOn { channel: ch, key: KeyNumber::new(*k), velocity: Velocity::new(vel) },
                });
            }
            if (pos..pos + BLOCK).contains(&hold_frames) {
                ev.push(PluginMidiEvent {
                    offset: (hold_frames - pos) as u32,
                    message: MidiEvent::NoteOff { channel: ch, key: KeyNumber::new(*k), velocity: Velocity::new(0) },
                });
            }
        }
        if (pos..pos + BLOCK).contains(&hold_frames) {
            ev.push(PluginMidiEvent {
                offset: (hold_frames - pos) as u32,
                message: MidiEvent::NoteOff {
                    channel: ch,
                    key,
                    velocity: Velocity::new(0),
                },
            });
        }
        ev
    });
    plugin.deactivate();

    write_wav(&a.out, a.sr, &l, &r).expect("write wav");
    let mono: Vec<f32> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
    let peak = mono.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!(
        "wrote {} ({:.2}s @ {} Hz) patch={:?} rms={:.5} hold_rms={:.5} tail_rms={:.5} peak={:.4} centroid_hz={:.1}",
        a.out.display(),
        total as f64 / sr,
        a.sr,
        loaded.as_deref().unwrap_or("?"),
        rms(&mono),
        rms(&mono[..hold_frames.min(mono.len())]),
        rms(&mono[hold_frames.min(mono.len())..]),
        peak,
        spectral_centroid(&mono[..hold_frames.min(mono.len())], a.sr),
    );
}
