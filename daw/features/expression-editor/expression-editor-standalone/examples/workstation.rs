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

    let runner = match Runner::open(&args.source, &args.target, args.viewport(), args.mode) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let Some(standalone) = runner.daw.clone() else {
        eprintln!("the workstation needs a project — open a .rpp, not a demo scene");
        std::process::exit(1);
    };

    // The in-process daw facade the arrange + mixer panels read, and
    // live meters for the strips.
    if let Err(e) = bootstrap_daw_blocking(&standalone) {
        eprintln!("daw bootstrap failed: {e}");
        std::process::exit(1);
    }
    let track_count =
        daw::service::Tracks::all(&standalone, daw::service::ProjectContext::Current).len();
    standalone.set_meters(daw::standalone::metering::Meters::new(track_count));

    // Real playback: the audio engine renders the project graph into
    // the default output (PipeWire on Linux) and drives the transport
    // clock sample-accurately. Kept alive for the window's life —
    // dropping it stops the stream. Failure is not fatal: the soft
    // clock still moves the playhead, just silently, and a machine
    // with no output device should still open the editor.
    let project_guid =
        daw::service::Projects::info(&standalone, daw::service::ProjectContext::Current)
            .map(|i| i.guid)
            .unwrap_or_default();
    // Inside the bootstrap's runtime: the engine spawns tasks on
    // construction, and a plain `main` has no reactor of its own.
    match expression_editor_standalone::workstation::in_daw_runtime(|| {
        standalone.attach_audio_engine(&project_guid)
    }) {
        Ok(engine) => {
            Box::leak(Box::new(engine));
        }
        Err(e) => eprintln!("no audio engine ({e}); transport will run silent"),
    }

    println!("{} — workstation", runner.label);
    stage_workstation(
        runner.loaded.into_editor(),
        runner.host,
        (args.width as f64, args.height as f64),
    );

    let window = WindowAttributes::default()
        .with_title(format!("FastTrackStudio — {}", runner.label))
        .with_surface_size(LogicalSize::new(args.width as f64, args.height as f64));
    launch_cfg(
        WorkstationApp,
        vec![],
        vec![Box::new(Config::new().with_window_attributes(window))],
    );
}
