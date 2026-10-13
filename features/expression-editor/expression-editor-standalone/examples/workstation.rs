//! The workstation in a real window: arrangement + TCP over the
//! expression editor, mixer down the right — the whole project.
//!
//! ```sh
//! cargo run -p expression-editor-standalone --example workstation -- \
//!     song.rpp --drums --size 1920x1080
//! ```
//!
//! Same source arguments as `--example editor`; the difference is what
//! mounts around the document. Launch boilerplate only — everything
//! composable lives in `expression_editor_standalone::workstation`.

use dioxus_native::{Config, LogicalSize, WindowAttributes, launch_cfg};
use expression_editor_standalone::cli::ArgsError;
use expression_editor_standalone::workstation::{
    WorkstationApp, bootstrap_daw_blocking, stage_workstation,
};
use expression_editor_standalone::{Args, Runner};

fn main() {
    // First, so a panic anywhere below is recorded. `dx serve` captures
    // the child's console into its own TUI, so a crash in the window
    // otherwise reaches nobody; this writes it to
    // target/dioxus-mcp/events.jsonl, which `dioxus-mcp`'s
    // `runtime_events` reads back. Leaked deliberately: dropping the
    // handle stops the writer, and it should outlive every frame.
    //
    // Panics only unless `FTS_DIOXUS_PROBE=1`. The full probe records
    // every `dioxus_core` trace event, whose fields are whole `VNode`
    // trees — 18,000 events a second at idle, and a scrolled frame of
    // the drum stack costs tens of thousands. See `probe`.
    Box::leak(Box::new(expression_editor_standalone::probe::install()));
    let args = match Args::from_env() {
        Ok(a) => a,
        Err(e @ (ArgsError::Help | ArgsError::List)) => {
            print!("{e}");
            return;
        }
        Err(e) => {
            eprintln!("{e}\n\n{}", expression_editor_standalone::cli::USAGE);
            std::process::exit(2);
        }
    };

    // Two phases, so the window is not held hostage by the kit.
    //
    // Parsing the project and standing up its backend is what the
    // arrangement, mixer and transport need. Decoding eighteen mics'
    // worth of the whole song — 28.8 s of the 44 s this used to take —
    // is what only the drum editor needs, and it happens on a thread
    // while the window is already up.
    let Some(path) = args.source.rpp_path() else {
        eprintln!("the workstation needs a project — open a .rpp, not a demo scene");
        std::process::exit(1);
    };
    let label = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let path = path.to_path_buf();

    println!("{label} — workstation");
    // The window opens on nothing and fills in behind itself.
    //
    // Everything below used to run first: parsing the project, standing
    // up its backend, attaching audio, then decoding eighteen mics'
    // worth of the whole song to find the drum hits. Together that is
    // about forty-four seconds of blank screen. None of it needs to
    // happen before a window exists — the panels already have a state
    // for a project that has not arrived ("Opening the project…"),
    // because waveforms have always streamed in behind the first paint.
    stage_workstation(
        expression_editor_standalone::app::fallback(),
        None,
        (args.width as f64, args.height as f64),
    );
    let kit_folder = args.target.drums.clone().flatten();
    let viewport = args.viewport();
    std::thread::Builder::new()
        .name("fts-project-load".into())
        .spawn(move || {
            let opened = match Runner::open_rpp_project(&path) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("the project did not open: {e}");
                    return;
                }
            };
            // The facade the arrangement, mixer and transport read. They
            // wait for it rather than assuming it is there.
            if let Err(e) = bootstrap_daw_blocking(&opened.daw) {
                eprintln!("daw bootstrap failed: {e}");
                return;
            }
            let track_count =
                daw::service::Tracks::all(&opened.daw, daw::service::ProjectContext::Current).len();
            opened
                .daw
                .set_meters(daw::standalone::metering::Meters::new(track_count));

            // Real playback: the audio engine renders the project graph
            // into the default output and drives the transport clock
            // sample-accurately. Kept alive for the window's life —
            // dropping it stops the stream. Failure is not fatal: the
            // soft clock still moves the playhead, just silently.
            let project_guid =
                daw::service::Projects::info(&opened.daw, daw::service::ProjectContext::Current)
                    .map(|i| i.guid)
                    .unwrap_or_default();
            // Inside the bootstrap's runtime: the engine spawns tasks on
            // construction, and a plain thread has no reactor of its own.
            match expression_editor_standalone::workstation::in_daw_runtime(|| {
                opened.daw.attach_audio_engine(&project_guid)
            }) {
                Ok(engine) => {
                    Box::leak(Box::new(engine));
                }
                Err(e) => eprintln!("no audio engine ({e}); transport will run silent"),
            }

            // The kit last: it is the slowest part and the only one the
            // other panels do not need.
            match Runner::analyse_kit(&opened, kit_folder.as_deref(), viewport) {
                Ok((_, editor, host)) => {
                    expression_editor_standalone::workstation::publish_kit(editor, Some(host));
                }
                // A kit that will not analyse is not a reason to take the
                // window down: the arrangement and mixer are still real.
                Err(e) => eprintln!("the kit did not open ({e}); the rest of the window works"),
            }
        })
        .expect("spawn project load");

    let window = WindowAttributes::default()
        .with_title(format!("FastTrackStudio — {label}"))
        .with_surface_size(LogicalSize::new(args.width as f64, args.height as f64));
    launch_cfg(
        WorkstationApp,
        vec![],
        vec![Box::new(Config::new().with_window_attributes(window))],
    );
}
