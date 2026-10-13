//! Reference renderer for any VST3 instrument: load a bundle, set its
//! component state from a file (e.g. a plug-in chunk dumped from a Gig
//! Performer `.gig` by signal's `gig_extract dump`), play one note on a
//! chosen MIDI channel and write a 32-bit float WAV.
//!
//! ```text
//! cargo run --release -p daw-standalone --features vst3-host --example plugin_render -- \
//!     <bundle.vst3> <state.chunk | -> <out.wav> [--note 60 --vel 100 --ch 1 \
//!     --hold 2.0 --tail 1.0 --sr 48000 --preroll 20 --bpm 120]
//! ```
//!
//! The pre-roll pumps the run loop and renders silence so sample-based
//! instruments (Kontakt) finish loading before the note; `--preroll` is in
//! seconds.

use std::ffi::c_void;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use daw_proto::{Channel, KeyNumber, MidiEvent, Velocity};
use daw_standalone::audio_engine::vst3_host::{LoadedVst3Plugin, Vst3Host};
use daw_standalone::plugin::{PluginEvents, PluginMidiEvent};

const BLOCK: usize = 512;

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

#[cfg(not(target_os = "macos"))]
fn pump_run_loop(secs: f64) {
    std::thread::sleep(Duration::from_secs_f64(secs));
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
    bus: usize,
    realtime: Option<f64>,
    frames: usize,
    mut events_at: impl FnMut(usize) -> Vec<PluginMidiEvent>,
) -> (Vec<f32>, Vec<f32>) {
    let zeros = vec![0.0f32; BLOCK];
    let mut l = vec![0.0f32; frames];
    let mut r = vec![0.0f32; frames];
    let mut pos = 0;
    let t0 = Instant::now();
    while pos < frames {
        // Paced to the wall clock: a disk-streaming sampler (Kontakt DFD)
        // only keeps up when blocks arrive no faster than real time.
        if let Some(sr) = realtime {
            let due = Duration::from_secs_f64(pos as f64 / sr);
            if let Some(wait) = due.checked_sub(t0.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        let n = BLOCK.min(frames - pos);
        let midi = events_at(pos);
        let ev = PluginEvents {
            params: &[],
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
        // An aux pair (a multi-out slot) replaces the main one.
        if bus > 1 {
            let (al, ar) = p.aux_output(bus - 1, n).expect("aux bus");
            l[pos..pos + n].copy_from_slice(al);
            r[pos..pos + n].copy_from_slice(ar);
        }
        pos += n;
    }
    (l, r)
}

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || -> ! {
        eprintln!("usage: plugin_render <bundle.vst3> <state|-> <out.wav> [--note N --vel V --ch C --bus N --realtime --hold S --tail S --sr HZ --preroll S --bpm B]");
        std::process::exit(2)
    };
    if args.len() < 3 {
        usage();
    }
    let opt = |k: &str, d: f64| -> f64 {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let (note, vel, ch) = (opt("--note", 60.0) as u8, opt("--vel", 100.0) as u8, opt("--ch", 1.0) as u8);
    let (hold, tail, sr, preroll) = (opt("--hold", 2.0), opt("--tail", 1.0), opt("--sr", 48000.0), opt("--preroll", 20.0));
    let t0 = Instant::now();
    let host = Vst3Host::new();
    let mut plugin = host.load(&PathBuf::from(&args[0]), 0).expect("load plugin");
    eprintln!("[{:>6.2?}] loaded {}", t0.elapsed(), plugin.descriptor().name);
    if args[1] != "-" {
        let state = std::fs::read(&args[1]).expect("read state");
        set_component_state(&mut plugin, &state);
        pump_run_loop(1.0);
        eprintln!("[{:>6.2?}] setState {} bytes", t0.elapsed(), state.len());
    }
    // `--bus N`: the stereo output pair to capture (1 = main).
    // `--realtime`: pace the note at real time (streaming samplers).
    let realtime = args.iter().any(|a| a == "--realtime").then_some(sr);
    let bus = opt("--bus", 1.0).max(1.0) as usize;
    plugin.set_output_buses(bus as u32);
    plugin.prepare(sr, BLOCK as u32).expect("prepare");
    plugin.set_tempo(opt("--bpm", 120.0));
    let pre = (preroll * sr) as usize;
    let mut done = 0;
    while done < pre {
        let n = BLOCK.min(pre - done);
        render(&mut plugin, bus, None, n, |_| Vec::new());
        pump_run_loop(0.002);
        done += n;
    }
    eprintln!("[{:>6.2?}] pre-rolled {preroll} s", t0.elapsed());
    let hold_frames = (hold * sr) as usize;
    let total = hold_frames + (tail * sr) as usize;
    let chan = Channel::new(ch.saturating_sub(1));
    let (l, r) = render(&mut plugin, bus, realtime, total, |pos| {
        let mut ev = Vec::new();
        if pos == 0 {
            ev.push(PluginMidiEvent { offset: 0, message: MidiEvent::NoteOn { channel: chan, key: KeyNumber::new(note), velocity: Velocity::new(vel) } });
        }
        if (pos..pos + BLOCK).contains(&hold_frames) {
            ev.push(PluginMidiEvent { offset: (hold_frames - pos) as u32, message: MidiEvent::NoteOff { channel: chan, key: KeyNumber::new(note), velocity: Velocity::new(0) } });
        }
        ev
    });
    write_wav(&PathBuf::from(&args[2]), sr as u32, &l, &r).expect("write wav");
    let peak = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
    eprintln!("[{:>6.2?}] wrote {} ({:.2} s, peak {:.3})", t0.elapsed(), args[2], total as f64 / sr, peak);
    plugin.deactivate();
}
