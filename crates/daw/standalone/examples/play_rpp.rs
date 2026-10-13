//! Load an RPP project, decode its audio sources, and play it through
//! the default cpal output via `AudioEngine::attached_to` — with a
//! line-based transport prompt on stdin.
//!
//! Run with (release, so the audio callback keeps up):
//!
//! ```bash
//! cargo run --release -p daw-standalone --features rpp-loader --example play_rpp -- /path/to/song.rpp
//! ```
//!
//! Options (before or after the path):
//!
//! - `--rate <hz>`     request a device sample rate (applied to the device)
//! - `--buffer <n>`    request a device buffer size in frames
//! - `--device <name>` output device by name substring
//! - `--in <name>`     input device by name substring (duplex)
//! - `--duplex`        use the low-latency duplex engine (one realtime
//!                     callback for input and output) instead of cpal
//! - `--list-devices`  print output devices and what they accept, then exit
//!
//! Pipeline:
//! 1. `project_loader::load_rpp_via_bay` parses the file + materializes
//!    audio sources through a project-relative resolver.
//! 2. `Standalone::attach_audio_engine(guid)` builds a cpal output
//!    stream whose callback drives `ProjectRenderer` every block.
//! 3. The prompt drives the `Transport` service.
//!
//! The transport engine spawns its tasks via `architect::platform::spawn`,
//! which on native is `tokio::spawn` — so everything runs inside a tokio
//! runtime, or attaching the engine panics with "no reactor running".

use std::io::{BufRead, Write};
use std::path::PathBuf;

use daw_audio_io::AudioIoPrefs;
use daw_proto::ProjectContext;
use daw_proto::transport::service::Transport;
use daw_standalone::media_bay::ProjectRelativeResolver;
use daw_standalone::project_loader::load_rpp_via_bay;
use daw_standalone::sync::Standalone;

const HELP: &str = "\
commands:
  p | space      play / pause
  s              stop and return to start
  g <time>       go to time (seconds, or m:ss)
  + <sec> / - <sec>
                 nudge forward / back (default 5 s)
  l <a> <b>      loop between two times, and turn looping on
  l              turn looping off
  t <bpm>        set tempo
  r <rate>       set play rate (1.0 = normal)
  ?              show transport state
  d              list output devices and what they accept
  h              this help
  q              quit";

fn main() -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let _guard = rt.enter();

    let args = Args::parse()?;
    if args.list_devices {
        list_devices();
        return Ok(());
    }
    let path = args.path.ok_or(USAGE)?;
    let rpp_path = PathBuf::from(&path);
    let rpp_text = std::fs::read_to_string(&rpp_path).map_err(|e| e.to_string())?;
    let project_dir = rpp_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let daw = Standalone::new();
    let project_name = rpp_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");

    // Install a project-relative filesystem resolver on the bay.
    // RPP source paths are usually relative to the project dir;
    // `ProjectRelativeResolver` handles that uniformly. On WASM the
    // app installs a JS-backed resolver here instead.
    daw.media_bay()
        .set_file_resolver(Box::new(ProjectRelativeResolver::new(project_dir.clone())));

    println!("Loading {path}...");
    let (proj, audio) = load_rpp_via_bay(
        &daw,
        project_name,
        rpp_path.to_string_lossy().as_ref(),
        &rpp_text,
    )?;

    println!(
        "  tracks={} items={} takes={} markers={} regions={} tempo_points={} hw_outs={}",
        proj.track_count,
        proj.item_count,
        proj.take_count,
        proj.marker_count,
        proj.region_count,
        proj.tempo_point_count,
        proj.hw_output_count,
    );
    println!(
        "  decoded {} audio sources ({} failed, {} no source)",
        audio.loaded,
        audio.failed.len(),
        audio.skipped_no_source,
    );
    for (take, err) in &audio.failed {
        eprintln!("    ! {take}: {err}");
    }

    // Attach the cpal audio engine. The output callback now renders
    // the loaded project every block. Dropping `_engine` stops audio.
    let hint = |e: String| format!("{e} (try --list-devices for what the device accepts)");
    // Held for its lifetime: dropping either engine stops audio.
    let (_engine, stats, rate, latency): (Box<dyn std::any::Any>, _, _, _) = if args.duplex {
        let engine = daw
            .attach_duplex_engine(&proj.project_guid, &args.prefs)
            .map_err(hint)?;
        let (stats, rate, latency) = (
            Some(engine.stats()),
            engine.sample_rate(),
            engine.latency_frames(),
        );
        (Box::new(engine), stats, rate, latency)
    } else {
        let engine = daw
            .attach_audio_engine_with_prefs(&proj.project_guid, &args.prefs)
            .map_err(hint)?;
        let (stats, rate) = (engine.stats(), engine.sample_rate());
        (Box::new(engine), stats, rate, None)
    };
    let ctx = ProjectContext::Project(proj.project_guid.clone());

    println!("{HELP}");
    // The first callback reports the block size the device actually runs at.
    std::thread::sleep(std::time::Duration::from_millis(200));
    if let Some(stats) = &stats {
        let frames = stats
            .block_frames
            .load(std::sync::atomic::Ordering::Relaxed);
        println!(
            "  {} engine: {rate} Hz, {frames}-frame blocks ({:.2} ms)",
            if args.duplex { "duplex" } else { "cpal" },
            frames as f64 * 1000.0 / rate as f64
        );
    }
    if let Some((i, o)) = latency {
        println!(
            "  hardware round trip: {} frames ({:.2} ms)",
            i + o,
            (i + o) as f64 * 1000.0 / rate as f64
        );
    }
    status(&daw, &ctx);
    prompt();

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        match run_command(&daw, &ctx, &line) {
            Ok(Flow::Quit) => break,
            Ok(Flow::Continue) => {}
            Err(e) => eprintln!("  ! {e}"),
        }
        prompt();
    }

    let _ = Transport::stop(&daw, ctx);
    if let Some(stats) = &stats {
        use std::sync::atomic::Ordering::Relaxed;
        let (_, peak_ms) = stats.render_ms();
        let frames = stats.block_frames.load(Relaxed);
        println!(
            "  {} blocks, {} over budget, {} xruns; render mean {:.3} ms, peak {:.3} ms (budget {:.3} ms)",
            stats.calls.load(Relaxed),
            stats.over_budget.load(Relaxed),
            stats.xruns.load(Relaxed),
            stats.mean_render_ms(),
            peak_ms,
            frames as f64 * 1000.0 / rate as f64,
        );
    }
    Ok(())
}

enum Flow {
    Continue,
    Quit,
}

fn run_command(daw: &Standalone, ctx: &ProjectContext, line: &str) -> Result<Flow, String> {
    let err = |e| format!("{e:?}");
    // A bare space is the play/pause key; everything else is trimmed.
    let (cmd, args) = if line == " " {
        ("p", Vec::new())
    } else {
        let mut words = line.split_whitespace();
        match words.next() {
            Some(cmd) => (cmd, words.collect::<Vec<_>>()),
            None => return Ok(Flow::Continue),
        }
    };
    match (cmd, args.as_slice()) {
        ("p", []) => Transport::play_pause(daw, ctx.clone()).map_err(err)?,
        ("s", []) => {
            Transport::stop(daw, ctx.clone()).map_err(err)?;
            Transport::goto_start(daw, ctx.clone()).map_err(err)?;
        }
        ("g", [t]) => Transport::set_position(daw, ctx.clone(), parse_time(t)?).map_err(err)?,
        ("+" | "-", rest) => {
            let step = match rest {
                [] => 5.0,
                [t] => parse_time(t)?,
                _ => return Err(format!("usage: {cmd} [seconds]")),
            };
            let step = if cmd == "-" { -step } else { step };
            let to = (Transport::get_position(daw, ctx.clone()) + step).max(0.0);
            Transport::set_position(daw, ctx.clone(), to).map_err(err)?;
        }
        ("l", []) => Transport::set_loop(daw, ctx.clone(), false).map_err(err)?,
        ("l", [a, b]) => {
            let (a, b) = (parse_time(a)?, parse_time(b)?);
            if b <= a {
                return Err("loop end must be after its start".into());
            }
            Transport::set_time_selection(daw, ctx.clone(), a, b).map_err(err)?;
            Transport::set_loop(daw, ctx.clone(), true).map_err(err)?;
        }
        ("t", [bpm]) => {
            let bpm = bpm.parse::<f64>().map_err(|e| e.to_string())?;
            Transport::set_tempo(daw, ctx.clone(), bpm).map_err(err)?;
        }
        ("r", [rate]) => {
            let rate = rate.parse::<f64>().map_err(|e| e.to_string())?;
            Transport::set_playrate(daw, ctx.clone(), rate).map_err(err)?;
        }
        ("?", []) => {}
        ("d", []) => list_devices(),
        ("h", []) => println!("{HELP}"),
        ("q", []) => return Ok(Flow::Quit),
        _ => return Err(format!("unknown command `{}` (h for help)", line.trim())),
    }
    status(daw, ctx);
    Ok(Flow::Continue)
}

const USAGE: &str = "usage: play_rpp [--rate <hz>] [--buffer <frames>] [--device <name>] \
     [--in <name>] [--duplex] [--list-devices] <rpp-file>";

struct Args {
    path: Option<String>,
    prefs: AudioIoPrefs,
    list_devices: bool,
    duplex: bool,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = Args {
            path: None,
            prefs: AudioIoPrefs::default(),
            list_devices: false,
            duplex: false,
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            let mut value = |flag: &str| it.next().ok_or(format!("{flag} needs a value\n{USAGE}"));
            match arg.as_str() {
                "--rate" => args.prefs.sample_rate = parse_u32(&value("--rate")?)?,
                "--buffer" => args.prefs.buffer_size = parse_u32(&value("--buffer")?)?,
                "--device" => args.prefs.output_device = value("--device")?,
                "--in" => args.prefs.input_device = value("--in")?,
                "--duplex" => args.duplex = true,
                "--list-devices" => args.list_devices = true,
                flag if flag.starts_with("--") => {
                    return Err(format!("unknown option {flag}\n{USAGE}"));
                }
                _ => args.path = Some(arg),
            }
        }
        Ok(args)
    }
}

fn parse_u32(s: &str) -> Result<u32, String> {
    s.parse()
        .map_err(|_| format!("`{s}` is not a whole number"))
}

fn list_devices() {
    let host = daw_audio_io::audio_host();
    for dev in daw_audio_io::output_devices(&host) {
        match daw_audio_io::device_caps(&host, Some(&dev.name), false) {
            Ok(caps) => println!(
                "  {}  ({} ch)  rates {:?}  buffer {}  [now {} Hz]",
                caps.name,
                dev.channels,
                caps.sample_rates,
                caps.buffer_range
                    .map(|(a, b)| format!("{a}..={b}"))
                    .unwrap_or_else(|| "?".into()),
                caps.default_sample_rate,
            ),
            Err(e) => println!("  {}  ({e})", dev.name),
        }
    }
}

/// `90`, `90.5` or `1:30` → seconds.
fn parse_time(s: &str) -> Result<f64, String> {
    let bad = || format!("bad time `{s}` (use seconds or m:ss)");
    match s.split_once(':') {
        Some((m, sec)) => {
            let m = m.parse::<f64>().map_err(|_| bad())?;
            let sec = sec.parse::<f64>().map_err(|_| bad())?;
            Ok(m * 60.0 + sec)
        }
        None => s.parse::<f64>().map_err(|_| bad()),
    }
}

fn fmt_time(secs: f64) -> String {
    let m = (secs / 60.0).floor();
    format!("{}:{:06.3}", m as u64, secs - m * 60.0)
}

fn status(daw: &Standalone, ctx: &ProjectContext) {
    let pos = Transport::get_position(daw, ctx.clone());
    let state = Transport::get_play_state(daw, ctx.clone());
    let tempo = Transport::get_tempo(daw, ctx.clone());
    let rate = Transport::get_playrate(daw, ctx.clone());
    let looping = match Transport::get_time_selection(daw, ctx.clone()) {
        Some(sel) if Transport::is_looping(daw, ctx.clone()) => format!(
            "loop {}–{}",
            fmt_time(sel.start_seconds),
            fmt_time(sel.end_seconds)
        ),
        _ => "loop off".into(),
    };
    println!(
        "  {state:?}  {}  {tempo:.2} bpm  rate {rate:.2}  {looping}",
        fmt_time(pos)
    );
}

fn prompt() {
    print!("> ");
    let _ = std::io::stdout().flush();
}
