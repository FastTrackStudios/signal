//! Open the platform duplex backend and report what it is doing: block
//! size, render time, overruns, xruns, reported round-trip latency, and the
//! peak level on each captured channel.
//!
//! Output is silent unless `--monitor <ch>` is given, which passes that
//! input channel to outputs 1/2 — mind feedback on speakers.
//!
//! ```bash
//! cargo run --release -p daw-audio-io --example duplex_probe -- \
//!     --in "Thunderbolt" --out "Thunderbolt" --rate 48000 --buffer 32 --inputs 2
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use daw_audio_io::duplex::{Backend, DuplexBackend, DuplexConfig, ProcessBlock};

fn main() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let mut cfg = DuplexConfig {
        name: "FTS duplex probe".into(),
        inputs: 2,
        outputs: 2,
        ..Default::default()
    };
    let (mut rate, mut buffer, mut secs, mut monitor) = (48_000u32, 64u32, 5u64, None::<usize>);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        let num = |s: String| s.parse::<u64>().map_err(|_| format!("bad number `{s}`"));
        match arg.as_str() {
            "--in" => cfg.input_device = Some(value()?),
            "--out" => cfg.output_device = Some(value()?),
            "--rate" => rate = num(value()?)? as u32,
            "--buffer" => buffer = num(value()?)? as u32,
            "--inputs" => cfg.inputs = num(value()?)? as usize,
            "--secs" => secs = num(value()?)?,
            "--monitor" => monitor = Some(num(value()?)? as usize),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    cfg.latency = Some((buffer, rate));

    // Per-input peak, written by the callback as f32 bits.
    let peaks: Arc<Vec<AtomicU32>> = Arc::new((0..cfg.inputs).map(|_| AtomicU32::new(0)).collect());
    let p = peaks.clone();
    let backend = Backend::start(
        cfg.clone(),
        Box::new(move |b: &mut ProcessBlock| {
            for (c, ch) in b.inputs.iter().enumerate() {
                let peak = ch.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                let cell = &p[c];
                let prev = f32::from_bits(cell.load(Ordering::Relaxed));
                cell.store(prev.max(peak).to_bits(), Ordering::Relaxed);
            }
            if let Some(src) = monitor.and_then(|m| b.inputs.get(m)).copied() {
                for out in b.outputs.iter_mut().take(2) {
                    out.copy_from_slice(src);
                }
            }
        }),
    )?;

    let rate = backend.sample_rate();
    if let Some((i, o)) = backend.latency_frames() {
        let ms = |f: u32| f as f64 * 1000.0 / rate as f64;
        println!(
            "latency: in {i} + out {o} frames = {} frames round trip ({:.2} ms at {rate} Hz)",
            i + o,
            ms(i + o)
        );
    }
    let stats = backend.stats();
    for _ in 0..secs {
        std::thread::sleep(Duration::from_secs(1));
        let (_, peak_ms) = stats.render_ms();
        let levels: Vec<String> = peaks
            .iter()
            .map(|c| {
                let v = f32::from_bits(c.swap(0, Ordering::Relaxed));
                if v > 0.0 {
                    format!("{:.1}", 20.0 * v.log10())
                } else {
                    "-inf".into()
                }
            })
            .collect();
        println!(
            "calls {:>6}  block {:>4}  mean {:.3} ms  peak {:.3} ms  over-budget {}  xruns {}  in dBFS [{}]",
            stats.calls.load(Ordering::Relaxed),
            stats.block_frames.load(Ordering::Relaxed),
            stats.mean_render_ms(),
            peak_ms,
            stats.over_budget.load(Ordering::Relaxed),
            stats.xruns.load(Ordering::Relaxed),
            levels.join(", "),
        );
    }
    drop(backend);
    println!("stopped cleanly");
    Ok(())
}
