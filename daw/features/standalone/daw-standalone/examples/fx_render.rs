//! Offline renderer for any VST3 *effect*: a thin CLI over
//! [`daw_standalone::offline_fx`]. Feed a WAV through it, set parameters
//! by name, write the result as 32-bit float.
//!
//! ```text
//! # What can be set (selector positions are spelled out):
//! cargo run --release -p daw-standalone --features vst3-host --example fx_render -- \
//!     <bundle.vst3> --list
//!
//! # Render (`=` display text, `:=` plain value, `~=` normalized 0..1):
//! cargo run --release -p daw-standalone --features vst3-host --example fx_render -- \
//!     <bundle.vst3> <in.wav> <out.wav> --set "EFFECT TYPE=CLOUD" --set "MIX~=1" \
//!     [--tail 8] [--preroll 2]
//! ```

use std::path::Path;

use daw_standalone::offline_fx::{self, RenderOptions};
use daw_standalone::plugin::PluginInstance;

fn read_wav(path: &Path) -> (u32, Vec<f32>, Vec<f32>) {
    let b = std::fs::read(path).expect("read input");
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (mut chans, mut sr, mut bits, mut fmt, mut data) = (1usize, 48_000u32, 32u16, 3u16, &b[0..0]);
    let mut i = 12;
    while i + 8 <= b.len() {
        let len = u32_at(i + 4) as usize;
        match &b[i..i + 4] {
            b"fmt " => {
                fmt = u16_at(i + 8);
                chans = usize::from(u16_at(i + 10));
                sr = u32_at(i + 12);
                bits = u16_at(i + 22);
                // WAVE_FORMAT_EXTENSIBLE: the real format is the sub-format.
                if fmt == 0xFFFE && len >= 26 {
                    fmt = u16_at(i + 32);
                }
            }
            b"data" => data = &b[i + 8..(i + 8 + len).min(b.len())],
            _ => {}
        }
        i += 8 + len + (len & 1);
    }
    let w = usize::from(bits / 8);
    let s = |x: &[u8]| match (fmt, bits) {
        (3, 32) => f32::from_le_bytes([x[0], x[1], x[2], x[3]]),
        (1, 16) => f32::from(i16::from_le_bytes([x[0], x[1]])) / 32768.0,
        (1, 24) => (i32::from_le_bytes([0, x[0], x[1], x[2]]) >> 8) as f32 / 8_388_608.0,
        _ => panic!("unsupported WAV format {fmt}/{bits}"),
    };
    let frames = data.len() / (w * chans);
    let l = (0..frames).map(|f| s(&data[f * w * chans..])).collect();
    let r = (0..frames).map(|f| s(&data[f * w * chans + w * (chans.min(2) - 1)..])).collect();
    (sr, l, r)
}

fn write_wav(path: &Path, sr: u32, l: &[f32], r: &[f32]) {
    let data_len = (l.len() * 8) as u32;
    let mut b = Vec::with_capacity(data_len as usize + 44);
    for chunk in [&b"RIFF"[..], &(36 + data_len).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes(), &3u16.to_le_bytes(),
        &2u16.to_le_bytes(), &sr.to_le_bytes(), &(sr * 8).to_le_bytes(), &8u16.to_le_bytes(), &32u16.to_le_bytes(),
        b"data", &data_len.to_le_bytes()]
    {
        b.extend_from_slice(chunk);
    }
    for (a, c) in l.iter().zip(r) {
        b.extend_from_slice(&a.to_le_bytes());
        b.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(path, b).expect("write wav");
}

fn list(plugin: &mut dyn PluginInstance) {
    for info in plugin.params() {
        let cur = plugin.param_value(info.id).unwrap_or(info.default);
        let text = plugin.value_to_text(info.id, cur).unwrap_or_default();
        let lo = plugin.value_to_text(info.id, info.min).unwrap_or_default();
        let hi = plugin.value_to_text(info.id, info.max).unwrap_or_default();
        println!("{:>6}  {:<26} {:>12}   [{} .. {}]", info.id, info.name, text.trim(), lo.trim(), hi.trim());
        if let Some(pos) = offline_fx::param_positions(plugin, &info) {
            let names: Vec<&str> = pos.iter().map(|(t, _)| t.as_str()).collect();
            println!("{:>6}  {:<26} {}", "", "", names.join(" | "));
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str, d: f64| -> f64 {
        args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(d)
    };
    let Some(bundle) = args.first() else {
        eprintln!("usage: fx_render <bundle.vst3> --list | <in.wav> <out.wav> [--set SPEC]... [--tail S] [--preroll S]");
        std::process::exit(2)
    };
    let mut plugin = offline_fx::load_vst3(Path::new(bundle), 0).expect("load plugin");
    eprintln!("loaded {}", plugin.descriptor().name);
    if args.get(1).map(String::as_str) == Some("--list") {
        list(&mut *plugin);
        return;
    }
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("need <in.wav> <out.wav>");
        std::process::exit(2)
    };
    let specs: Vec<&str> = args.windows(2).filter(|w| w[0] == "--set").map(|w| w[1].as_str()).collect();
    let params = offline_fx::resolve_all(&mut *plugin, &specs).expect("resolve --set");
    for ((id, plain), spec) in params.iter().zip(&specs) {
        eprintln!("  {spec:<28} -> id {id} = {plain:.5} ({})", plugin.value_to_text(*id, *plain).unwrap_or_default().trim());
    }
    let (sr, l, r) = read_wav(Path::new(input));
    let opts = RenderOptions {
        sample_rate: f64::from(sr),
        preroll_secs: opt("--preroll", 2.0),
        tail_secs: opt("--tail", 6.0),
        params,
        pump_run_loop: true,
        ..RenderOptions::default()
    };
    let (ol, or) = offline_fx::render(&mut *plugin, &l, &r, &opts).expect("render");
    write_wav(Path::new(output), sr, &ol, &or);
    let peak = ol.iter().chain(&or).fold(0.0f32, |m, v| m.max(v.abs()));
    eprintln!("wrote {output} ({:.2} s, peak {peak:.3}, latency {} smp)", ol.len() as f64 / f64::from(sr), plugin.latency());
    plugin.deactivate();
}
