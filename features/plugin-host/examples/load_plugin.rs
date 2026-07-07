//! Load an external plugin (CLAP / VST3), prepare it, render one MIDI-driven
//! block, and report what happened — the smoke test for hosting third-party
//! instruments (e.g. Pianoteq) inside Signal.
//!
//! ```text
//! cargo run -p signal-plugin-host --features vst3-host --example load_plugin -- \
//!     "$HOME/.vst3/Pianoteq 9.vst3"
//! ```

use signal_plugin_host::{HostedPlugin, PluginMidiEvent};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: load_plugin <path-to-.clap-or-.vst3>");

    let mut plugin = match HostedPlugin::load(&path) {
        Ok(Some(p)) => p,
        Ok(None) => {
            eprintln!("{path}: resolved to the synthetic backend (nothing to host)");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{path}: load failed: {e}");
            std::process::exit(1);
        }
    };

    let d = plugin.descriptor().clone();
    println!("loaded  : {} — {} ({:?})", d.name, d.vendor, d.format);
    println!("id      : {}", d.id);

    if let Err(e) = plugin.prepare(48_000.0, 512) {
        eprintln!("prepare failed: {e}");
        std::process::exit(1);
    }
    let params = plugin.params();
    println!("params  : {}", params.len());
    // `--params` dumps the full parameter surface (id, range, default,
    // current value, display text) as TSV — the Omnisphere calibration path.
    if std::env::args().any(|a| a == "--params") {
        for p in &params {
            let value = plugin.param_value(p.id).unwrap_or(p.default);
            let text = plugin.value_to_text(p.id, value).unwrap_or_default();
            println!(
                "PARAM\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                p.id, p.name, p.min, p.max, p.default, value, text
            );
        }
    } else {
        for p in params.iter().take(8) {
            println!("  [{:>5}] {}", p.id, p.name);
        }
    }
    println!("latency : {} frames", plugin.latency());

    // `--load-state <file>` restores a chunk before rendering (A/B path).
    // Runs BEFORE --save-state so the pair round-trips: what did the engine
    // keep / normalize from an injected state?
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--load-state") {
        let path = args.get(i + 1).expect("--load-state <file>");
        let bytes = std::fs::read(path).expect("read state");
        match plugin.load_state(&bytes) {
            Ok(()) => println!("state   : loaded {} bytes from {path}", bytes.len()),
            Err(e) => println!("state   : load failed: {e}"),
        }
        // Some engines (Omnisphere) rebuild parts of their mod graph only on
        // activation — cycle it so loaded state fully takes effect.
        if std::env::args().any(|a| a == "--reactivate") {
            plugin.deactivate();
            plugin
                .prepare(48_000.0, 512)
                .expect("re-prepare after load-state");
            println!("state   : reactivated");
        }
    }
    // `--save-state <file>` dumps the plugin's state chunk.
    if let Some(i) = args.iter().position(|a| a == "--save-state") {
        let path = args.get(i + 1).expect("--save-state <file>");
        match plugin.save_state() {
            Ok(state) => {
                std::fs::write(path, &state).expect("write state");
                println!("state   : {} bytes -> {path}", state.len());
            }
            Err(e) => println!("state   : save failed: {e}"),
        }
    }

    // Render MIDI through the plugin — proves the instrument makes sound and,
    // with `--render <wav>`, captures the audio (the A/B harness path).
    //   --note <n>   MIDI note (default 60)
    //   --secs <s>   total render length (default ~0.53 s), note held for half
    //   --render <f> write stereo f32 WAV at 48 kHz
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let note: u8 = flag("--note").and_then(|s| s.parse().ok()).unwrap_or(60);
    let secs: f32 = flag("--secs").and_then(|s| s.parse().ok()).unwrap_or(0.53);
    let render_path = flag("--render");

    let block = 512usize;
    let total_blocks = ((secs * 48_000.0) as usize / block).max(2);
    let off_block = total_blocks / 2;
    use daw::service::{Channel, KeyNumber, MidiEvent, Velocity};
    let note_on = [PluginMidiEvent {
        offset: 0,
        message: MidiEvent::NoteOn {
            channel: Channel::new(0),
            key: KeyNumber::new(note),
            velocity: Velocity::new(100),
        },
    }];
    let note_off = [PluginMidiEvent {
        offset: 0,
        message: MidiEvent::NoteOff {
            channel: Channel::new(0),
            key: KeyNumber::new(note),
            velocity: Velocity::new(0),
        },
    }];
    let mut inter = vec![0.0f32; block * 2];
    let mut peak = 0.0f32;
    let mut captured: Vec<f32> = Vec::new();
    for b in 0..total_blocks {
        let midi: &[PluginMidiEvent] = if b == 0 {
            &note_on
        } else if b == off_block {
            &note_off
        } else {
            &[]
        };
        inter.iter_mut().for_each(|s| *s = 0.0);
        if let Err(e) = plugin.process_interleaved(&mut inter, midi, &[]) {
            eprintln!("process failed: {e}");
            std::process::exit(1);
        }
        peak = peak.max(inter.iter().fold(0.0f32, |m, s| m.max(s.abs())));
        if render_path.is_some() {
            captured.extend_from_slice(&inter);
        }
    }
    println!(
        "peak    : {peak:.4} ({})",
        if peak > 1e-4 { "AUDIBLE" } else { "silent" }
    );
    if let Some(path) = render_path {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).expect("create wav");
        for s in &captured {
            w.write_sample(*s).expect("write sample");
        }
        w.finalize().expect("finalize wav");
        println!("render  : {} frames -> {path}", captured.len() / 2);
    }
}
