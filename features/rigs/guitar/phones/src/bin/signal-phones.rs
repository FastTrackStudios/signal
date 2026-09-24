//! `signal-phones` — the headphone mixer process (see the crate docs).
//!
//!   signal-phones [--state PATH] [--block FRAMES]
//!
//! Plays the monitor-mix input pair into the phones output pair at the
//! levels in the state file, until the rig turns it off (or it is killed).
//! Reopens the device when the routing changes, when the device goes away
//! and comes back, and when the audio stops flowing.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use daw_audio_io::duplex::{Backend, DuplexBackend, DuplexConfig, ProcessBlock};
use signal_phones::{MixerState, PhonesLink, claim, default_state_path, fader_gain};

/// How often the control loop looks at the file and the device.
const TICK: Duration = Duration::from_millis(100);
/// Audio that has not moved for this long is a stream to reopen.
const STALL: Duration = Duration::from_secs(2);
/// Between attempts to open a missing device.
const RETRY: Duration = Duration::from_secs(1);

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_ansi(false)
        .init();

    let mut state = default_state_path();
    let mut block = 128u32;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--state" => state = args.next().map(PathBuf::from).unwrap_or(state),
            "--block" => block = args.next().and_then(|v| v.parse().ok()).unwrap_or(block),
            other => {
                tracing::error!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    // One mixer at a time. A rig probing the lock at this instant would
    // make the first try fail, so give it a moment.
    let mut instance = None;
    for _ in 0..5 {
        match claim(&state) {
            Ok(Some(i)) => {
                instance = Some(i);
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                tracing::error!("phones mixer: cannot lock {}: {e}", state.display());
                std::process::exit(1);
            }
        }
    }
    let Some(_instance) = instance else {
        tracing::info!("phones mixer: another one is running on {}", state.display());
        return;
    };

    let link = match PhonesLink::open(&state) {
        Ok(l) => Arc::new(l),
        Err(e) => {
            tracing::error!("phones mixer: cannot open {}: {e}", state.display());
            std::process::exit(1);
        }
    };
    tracing::info!(pid = std::process::id(), state = %state.display(), "phones mixer: up");
    link.report(MixerState::Idle, 0, 0);

    let mut stream: Option<(Backend, u32)> = None;
    let mut last_try: Option<Instant> = None;
    let mut last_beat = (0u32, Instant::now());
    loop {
        if !link.enabled() {
            tracing::info!("phones mixer: turned off — exiting");
            break;
        }
        let seq = link.config_seq();
        let dead = stream.as_ref().is_some_and(|(b, s)| {
            *s != seq || b.stats().stream_state.load(Ordering::Relaxed) < 0
        });
        // Audio that stopped moving (the device's clock went away without
        // saying so) is a stream to reopen too.
        let beat = link.shared_heartbeat();
        if beat != last_beat.0 {
            last_beat = (beat, Instant::now());
        }
        let stalled = stream.is_some() && last_beat.1.elapsed() > STALL;
        if dead || stalled {
            if stalled {
                tracing::warn!("phones mixer: the audio stopped — reopening");
            } else {
                tracing::info!("phones mixer: routing changed or device lost — reopening");
            }
            stream = None;
            link.report(MixerState::Idle, 0, 0);
        }
        if stream.is_none() && last_try.is_none_or(|t| t.elapsed() >= RETRY) {
            last_try = Some(Instant::now());
            match open(&link, block) {
                Ok(b) => {
                    let rate = b.sample_rate();
                    let frames = b.stats().block_frames.load(Ordering::Relaxed);
                    link.report(MixerState::Streaming, rate, frames.max(block));
                    last_beat = (link.shared_heartbeat(), Instant::now());
                    stream = Some((b, seq));
                }
                Err(e) => {
                    link.report(MixerState::NoDevice, 0, 0);
                    tracing::debug!("phones mixer: {e}");
                }
            }
        }
        std::thread::sleep(TICK);
    }
    drop(stream);
    link.report(MixerState::Idle, 0, 0);
}

/// Open the device and play the mix.
fn open(link: &Arc<PhonesLink>, block: u32) -> Result<Backend, String> {
    let cfg = link.config();
    let (in_l, in_r) = (cfg.mix_in.0 as usize, cfg.mix_in.1 as usize);
    let (out_l, out_r) = (cfg.out.0 as usize, cfg.out.1 as usize);
    let device = Some(cfg.device.clone()).filter(|d| !d.is_empty());
    let dc = DuplexConfig {
        name: "Signal Phones".into(),
        inputs: in_l.max(in_r) + 1,
        outputs: out_l.max(out_r) + 1,
        // The rig owns the device's rate; ask only for a small block.
        latency: None,
        buffer: Some(block),
        input_device: device.clone(),
        output_device: device,
        allow_builtin_mic: false,
    };
    let l = link.clone();
    // Gains glide to their targets over ~10 ms, so a fader move does not
    // click.
    let (mut g_l, mut g_r) = (0.0f32, 0.0f32);
    let (mut m_l, mut m_r) = (0.0f32, 0.0f32);
    let backend = Backend::start(
        dc,
        Box::new(move |b: &mut ProcessBlock| {
            let (mix, vol) = l.levels();
            let target = fader_gain(mix) * fader_gain(vol);
            let n = b.frames;
            let coef = 1.0 - (-1.0 / (0.010 * 48_000.0f32)).exp();
            let (mut pk_l, mut pk_r) = (0.0f32, 0.0f32);
            for o in b.outputs.iter_mut() {
                o[..n].fill(0.0);
            }
            for f in 0..n {
                let x_l = b.inputs.get(in_l).map_or(0.0, |c| c[f]);
                let x_r = b.inputs.get(in_r).map_or(0.0, |c| c[f]);
                pk_l = pk_l.max(x_l.abs());
                pk_r = pk_r.max(x_r.abs());
                g_l += (target - g_l) * coef;
                g_r += (target - g_r) * coef;
                if let Some(o) = b.outputs.get_mut(out_l) {
                    o[f] = x_l * g_l;
                }
                if out_r != out_l {
                    if let Some(o) = b.outputs.get_mut(out_r) {
                        o[f] = x_r * g_r;
                    }
                }
            }
            // The meter holds a peak and falls ~20 dB/s from it.
            let rate = match l.shared_rate() {
                0 => 48_000,
                r => r,
            };
            let fall = signal_phones::meter_fall(rate, n);
            m_l = pk_l.max(m_l * fall);
            m_r = pk_r.max(m_r * fall);
            l.beat(m_l, m_r);
        }),
    )?;
    tracing::info!(
        device = %cfg.device,
        mix_in = ?(in_l + 1, in_r + 1),
        phones_out = ?(out_l + 1, out_r + 1),
        rate = backend.sample_rate(),
        "phones mixer: playing the mix"
    );
    Ok(backend)
}
