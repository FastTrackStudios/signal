//! fts — the one FastTrackStudio CLI.
//!
//! Mounts the domain CLIs as subcommands (`fts daw <...>` → `daw::cli`,
//! `fts keyflow <...>` → `keyflow-cli`) and manages the headless engines:
//! `fts template <...>` classifies names through the Dynamic Template
//! offline (no DAW involved).
//! `fts signal engine` runs the headless signal engine — the
//! `fasttrackstudio` binary in `--engine` mode (the rig core,
//! ws://:4040/vox), `fts session engine` runs the standalone session
//! engine in-process (ws://:3030/ws), `fts status` probes both ports.

use std::ffi::OsString;

use clap::{Parser, Subcommand};
use engine_launcher::{LaunchSource, SESSION_ENGINE, SIGNAL_ENGINE, probe, spawn};
use eyre::Result;

#[cfg(feature = "session")]
mod session_engine;
#[cfg(feature = "template")]
mod template;

#[derive(Parser)]
#[command(name = "fts", about = "FastTrackStudio — one CLI over the whole stack")]
struct Fts {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// The daw command tree (live-query REAPER / standalone) — `fts daw tracks`, ...
    #[cfg(feature = "daw")]
    #[command(disable_help_flag = true)]
    Daw {
        /// Arguments forwarded verbatim to the daw CLI.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// The keyflow chart CLI — `fts keyflow parse chart.kf`, ... (alias: kf)
    #[cfg(feature = "keyflow")]
    #[command(alias = "kf", disable_help_flag = true)]
    Keyflow {
        /// Arguments forwarded verbatim to the kf CLI.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Signal domain — the headless rig core.
    Signal {
        #[command(subcommand)]
        command: SignalCmd,
    },
    /// Session domain — the standalone setlist engine.
    Session {
        #[command(subcommand)]
        command: SessionCmd,
    },
    /// Dynamic Template, offline — classify names into folders and colours.
    #[cfg(feature = "template")]
    #[command(alias = "dynamic-template")]
    Template {
        #[command(subcommand)]
        command: template::TemplateCommand,
    },
    /// Probe the known engine ports and report up/down.
    Status,
}

#[derive(Subcommand)]
enum SignalCmd {
    /// Spawn the signal engine — `fasttrackstudio --engine` (guitar in →
    /// NAM chain → out, vox router on ws://:4040/vox). Detaches by default.
    Engine {
        /// Bind address for the engine (sets SIGNAL_ENGINE_ADDR).
        #[arg(long)]
        addr: Option<String>,
        /// Run attached to this terminal instead of detaching.
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the signal engine (the systemd user unit — same switch the
    /// desktop app uses; a stop is final, nothing restarts it).
    Stop,
    /// Signalpack tools: inspect / spec / zones / loops / check / extract /
    /// build / transcode — `fts signal pack inspect <pack> --json`, ...
    #[cfg(feature = "pack")]
    #[command(disable_help_flag = true)]
    Pack {
        /// Arguments forwarded verbatim to the pack CLI.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    /// Run the standalone session engine in-process: daw-standalone +
    /// SetlistService + stream pumps, served on ws://:3030/ws.
    Engine {
        /// Bind address (default 0.0.0.0:3030; SESSION_ENGINE_ADDR also honored).
        #[arg(long)]
        addr: Option<String>,
    },
}

fn main() -> Result<()> {
    // Console logs on STDERR (stdout is command output), plus telemetry:
    // Sentry (TASK_SENTRY_DSN) and OTLP export when
    // OTEL_EXPORTER_OTLP_ENDPOINT is set (http/protobuf → :4318).
    // Hand-composed (not `architect_telemetry::init_tracing_full`) to keep
    // the stderr writer. Guards live in main's scope so exporters flush on
    // exit — a CLI run is short, so the final flush is the export.
    let _sentry = architect_telemetry::init("fts-cli");
    let registry = {
        use tracing_subscriber::layer::SubscriberExt as _;
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    // symphonia logs per-stream demux INFO — noise during pack
                    // decode probes/transcodes.
                    .unwrap_or_else(|_| {
                        "info,symphonia_format_ogg=warn,symphonia_core=warn".into()
                    }),
            )
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(architect_telemetry::tracing_layer())
    };
    let _otel = {
        use tracing_subscriber::layer::SubscriberExt as _;
        use tracing_subscriber::util::SubscriberInitExt as _;
        match architect_telemetry::otel::init("fts-cli") {
            Some((guard, layers)) => {
                registry.with(layers).init();
                Some(guard)
            }
            None => {
                registry.init();
                None
            }
        }
    };

    match Fts::parse().command {
        #[cfg(feature = "daw")]
        Cmd::Daw { args } => {
            let argv = std::iter::once(OsString::from("daw")).chain(args);
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(daw::cli::cli_main(argv))
        }
        #[cfg(feature = "keyflow")]
        Cmd::Keyflow { args } => {
            let argv = std::iter::once(OsString::from("kf")).chain(args);
            keyflow_cli::cli_main(argv);
            Ok(())
        }
        Cmd::Signal {
            command: SignalCmd::Engine { addr, foreground },
        } => signal_engine(addr, foreground),
        Cmd::Signal {
            command: SignalCmd::Stop,
        } => signal_stop(),
        #[cfg(feature = "pack")]
        Cmd::Signal {
            command: SignalCmd::Pack { args },
        } => signal_sampler::pack_cli::cli_main(args),
        Cmd::Session {
            command: SessionCmd::Engine { addr },
        } => session_engine_cmd(addr),
        #[cfg(feature = "template")]
        Cmd::Template { command } => template::run(command),
        Cmd::Status => {
            status();
            Ok(())
        }
    }
}

/// `fts signal engine` — spawn `fasttrackstudio --engine` as a child process.
fn signal_engine(addr: Option<String>, foreground: bool) -> Result<()> {
    // Only meaningful on the default port — a custom --addr is the
    // caller saying "bind here", e.g. a second engine next to a live one.
    if addr.is_none() && probe(&SIGNAL_ENGINE) {
        println!(
            "signal engine already running — {}",
            SIGNAL_ENGINE.ws_url()
        );
        return Ok(());
    }

    // Default detached start goes through the systemd user unit when
    // installed (`just rig-install`) — same switch the desktop app uses:
    // crash-supervised while running, `fts signal stop` is final.
    if addr.is_none() && !foreground && engine_launcher::systemd_available(&SIGNAL_ENGINE) {
        engine_launcher::systemd_start(&SIGNAL_ENGINE)?;
        println!(
            "signal engine started (systemd user unit) — {}",
            SIGNAL_ENGINE.ws_url()
        );
        return Ok(());
    }

    // The engine reads SIGNAL_ENGINE_ADDR (legacy RIGD_ADDR still honored);
    // the rest of the environment propagates as-is.
    let mut env = Vec::new();
    if let Some(addr) = &addr {
        env.push(("SIGNAL_ENGINE_ADDR".to_string(), addr.clone()));
    }

    // supervise: false — the CLI manages the engine's lifecycle itself
    // (foreground = shared Ctrl-C group; background = detached), so it should
    // not self-reap on the CLI process exiting.
    let spawned = spawn(&SIGNAL_ENGINE, &[], &env, !foreground, false)?;
    let url = match &addr {
        Some(a) => format!("ws://{a}/vox"),
        None => SIGNAL_ENGINE.ws_url(),
    };
    match &spawned.source {
        LaunchSource::Binary(path) => eprintln!("spawned {}", path.display()),
        LaunchSource::Cargo => eprintln!("no fasttrackstudio binary found — `cargo run --release -p fasttrackstudio -- --engine` (dev fallback)"),
    }

    if foreground {
        // Attached: the child shares our process group, so Ctrl-C reaches
        // it directly; we just wait and mirror its exit status.
        println!("signal engine (foreground) — {url}");
        let mut child = spawned.child;
        let status = child.wait()?;
        if !status.success() {
            eyre::bail!("signal engine exited: {status}");
        }
        Ok(())
    } else {
        println!("signal engine started — pid {} — {url}", spawned.pid());
        Ok(())
    }
}

/// `fts signal stop` — stop the engine's systemd user unit.
fn signal_stop() -> Result<()> {
    if engine_launcher::systemd_active(&SIGNAL_ENGINE) {
        engine_launcher::systemd_stop(&SIGNAL_ENGINE)?;
        println!("signal engine stopped");
        return Ok(());
    }
    if probe(&SIGNAL_ENGINE) {
        eyre::bail!(
            "an engine is serving {} but not via the systemd unit — \
             stop it where it was started (desktop app child, foreground \
             terminal, or kill its pid)",
            SIGNAL_ENGINE.ws_url()
        );
    }
    println!("signal engine is not running");
    Ok(())
}

/// `fts session engine` — the in-process standalone session engine.
#[cfg(feature = "session")]
fn session_engine_cmd(addr: Option<String>) -> Result<()> {
    session_engine::run_blocking(addr)
}

#[cfg(not(feature = "session"))]
fn session_engine_cmd(_addr: Option<String>) -> Result<()> {
    eyre::bail!("fts was built without the `session` feature")
}

/// `fts status` — up/down per known engine port.
fn status() {
    for engine in [&SIGNAL_ENGINE, &SESSION_ENGINE] {
        let up = probe(engine);
        println!(
            "{:<8} {}  {}",
            engine.name.to_lowercase(),
            if up { "up  " } else { "down" },
            engine.ws_url(),
        );
    }
}
