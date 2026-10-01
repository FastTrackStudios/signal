//! `signal rig …` — the guitar rig's library, without the app.

use std::process::ExitCode;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum Command {
    /// Ask the running rig to apply every config file edited since it
    /// loaded it, now, and print what it did. (It also picks edits up on
    /// its own within a second; this is for "now, and tell me".) Reaches
    /// the rig through its config directory, so run it with the same
    /// `XDG_CONFIG_HOME` / `SIGNAL_RIG_DIR` as the app.
    Reload {
        /// Seconds to wait for the rig to answer.
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },
    /// Level every patch of a profile to the same loudness (writes each
    /// patch's `level_db`; the player's own `trim_db` is left alone).
    /// Restart the app afterwards — it reads the profile at load.
    Level {
        /// Profile name (e.g. Metal). Default: the active profile.
        profile: Option<String>,
        /// Measure and print, write nothing.
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
    },
    /// Regroup a profile into presets with snapshots, built from module
    /// presets (amp captures grouped by family, the pedal board as a Drive
    /// preset). Sounds are unchanged. Prints the proposal; `--write` saves.
    Migrate {
        profile: String,
        #[arg(long)]
        write: bool,
    },
    /// Level the building blocks on their own: every amp module snapshot
    /// through its own cab (to the rig's target), every drive option to unity
    /// at drive 0.5. Writes each block's Output Level. Run before
    /// `level-presets` and `level`.
    LevelModules {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
    },
    /// Name every preset for the gear it plays: dissolve the profile-named
    /// presets (`Worship Clean`, `Metal Rhythm`, …) into the amp presets
    /// their snapshots play, and repoint every profile at them. Sounds are
    /// unchanged. Dry run unless `--write`.
    RegroupPresets {
        #[arg(long)]
        write: bool,
    },
    /// List the module presets as the rig loads them (modules.styx).
    Modules,
    /// Make a profile pure references: each patch becomes a snapshot of a
    /// "<profile> <stack>" preset (its overrides, its amp mapped through
    /// `--map`, the board as a Pedalboard snapshot); the profile keeps only
    /// stacks and references. Dry run unless `--write`.
    Recompose {
        profile: String,
        /// Lines of `old capture = Amp preset / snapshot`.
        #[arg(long)]
        map: std::path::PathBuf,
        #[arg(long)]
        write: bool,
    },
    /// Level every preset snapshot to the same loudness (writes each
    /// snapshot's `level_db` into presets.styx). The rig picks it up on the
    /// next change — no restart.
    /// Write the rig as the browser plays it: rig.json (every patch's
    /// resolved chain) plus its models and IRs under content-addressed keys.
    WebBundle {
        out: std::path::PathBuf,
        /// Only these profiles (default: all).
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Dial each preset snapshot's Post Comp threshold to its compressor
    /// preset's target gain reduction, on that snapshot's own amp (writes a
    /// `Post Comp threshold` override per snapshot into presets.styx). The
    /// Pre Comp is never dialled: it hears only the guitar. Run
    /// `level-presets` after.
    DialPostComp {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
        /// Render threads (0 = every core).
        #[arg(long, default_value_t = 0)]
        threads: usize,
        /// Dial only this preset's snapshots (after changing its amps).
        #[arg(long)]
        preset: Option<String>,
    },
    LevelPresets {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
        /// Render threads (0 = every core).
        #[arg(long, default_value_t = 0)]
        threads: usize,
    },
    /// Freeze a Core snapshot into NAM captures: render NAM's official
    /// training signal through the Core, train it with the NAM trainer's
    /// standard pipeline (the A2 model), then play a guitar through the
    /// live and the frozen Core and report the difference. The original
    /// stays; the captures are saved beside it. `--play` also switches the
    /// snapshot to the frozen one.
    Freeze {
        /// The Core preset (e.g. "John Mayer").
        preset: String,
        /// Its snapshot; every snapshot when left out.
        snapshot: Option<String>,
        /// Training epochs (the trainer's default is 100).
        #[arg(long, default_value_t = 100)]
        epochs: u32,
        /// Play the frozen Core once it's made.
        #[arg(long)]
        play: bool,
        /// Re-check existing captures against the live Core; train nothing.
        #[arg(long)]
        check_only: bool,
    },
    /// Flip a Core snapshot between its live settings and its frozen
    /// captures.
    Frozen {
        preset: String,
        snapshot: String,
        /// on | off
        state: String,
    },
}

pub fn run(command: Command) -> ExitCode {
    match command {
        Command::Reload { timeout } => reload(timeout),
        Command::WebBundle { out, profiles } => {
            match signal_guitar::web_bundle::export(&out, &profiles) {
                Ok((bundle, skipped)) => {
                    let patches: usize = bundle.profiles.iter().map(|p| p.patches.len()).sum();
                    let bytes: u64 = bundle.assets.iter().map(|a| a.bytes).sum();
                    println!(
                        "{} profiles, {patches} patches, {} assets ({:.1} MB) → {}",
                        bundle.profiles.len(),
                        bundle.assets.len(),
                        bytes as f64 / 1e6,
                        out.display()
                    );
                    if skipped.plugin_blocks > 0 {
                        eprintln!(
                            "skipped {} plugin blocks (no plugin host in a browser)",
                            skipped.plugin_blocks
                        );
                    }
                    for m in &skipped.missing {
                        eprintln!("missing, block dropped: {m}");
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Recompose {
            profile,
            map,
            write,
        } => {
            let Ok(text) = std::fs::read_to_string(&map) else {
                eprintln!("cannot read {}", map.display());
                return ExitCode::FAILURE;
            };
            let amp_map = signal_guitar::compose::parse_amp_map(&text);
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let Some(mut def) = lib
                .profiles
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&profile))
                .cloned()
            else {
                eprintln!("no profile named {profile:?}");
                return ExitCode::FAILURE;
            };
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            let unmapped = signal_guitar::compose::recompose(
                &mut def,
                &mut comp,
                &amp_map,
                &lib.drive_presets,
            );
            if !unmapped.is_empty() {
                eprintln!(
                    "no mapping for the amp of: {} — add them to the map",
                    unmapped.join(", ")
                );
                return ExitCode::FAILURE;
            }
            for p in &def.patches {
                println!("  {:<22} → {} / {}", p.name, p.rig_preset, p.snapshot);
            }
            // Every pick must land on something real.
            let mut bad = 0;
            for p in comp
                .presets
                .iter()
                .filter(|p| p.name.starts_with(&def.name))
            {
                for snap in &p.snapshots {
                    for pick in &snap.modules {
                        if !comp.module(&pick.module, &pick.preset).is_some_and(|m| {
                            m.snapshots
                                .iter()
                                .any(|s| s.name.eq_ignore_ascii_case(&pick.snapshot))
                        }) {
                            bad += 1;
                            eprintln!(
                                "  {} / {}: no {} {} / {}",
                                p.name, snap.name, pick.module, pick.preset, pick.snapshot
                            );
                        }
                    }
                }
            }
            if bad > 0 {
                return ExitCode::FAILURE;
            }
            if write {
                signal_guitar::library::RigLibrary::save_compositions(&comp);
                signal_guitar::library::RigLibrary::save_profile(&def);
                println!("written.");
            } else {
                println!("(dry run — pass --write to save)");
            }
            ExitCode::SUCCESS
        }
        Command::LevelModules { dry_run, sample_rate } => {
            let started = std::time::Instant::now();
            let results = signal_guitar::levelling::level_modules(sample_rate, dry_run);
            let mut failed = 0;
            for r in &results {
                match r.lufs {
                    Some(l) => println!("{:<60} {l:>7.1} LUFS  →  {:+.1} dB", r.what, r.level_db),
                    None => {
                        failed += 1;
                        println!("{:<60} did not render", r.what);
                    }
                }
            }
            println!(
                "\n{} modules in {:.0}s, {failed} failed{}",
                results.len(),
                started.elapsed().as_secs_f32(),
                if dry_run { " (dry run — nothing written)" } else { "" }
            );
            ExitCode::SUCCESS
        }
        Command::RegroupPresets { write } => {
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            let mut profiles = lib.profiles.clone();
            let moved = signal_guitar::compose::regroup_by_gear(&mut comp, &mut profiles);
            let mut last = String::new();
            for m in &moved {
                if m.from_preset != last {
                    println!("{}", m.from_preset);
                    last.clone_from(&m.from_preset);
                }
                println!(
                    "  {:<22} → {} · {}{}",
                    m.from_snapshot,
                    m.to_preset,
                    m.to_snapshot,
                    if m.reused { "  (same as existing)" } else { "" }
                );
            }
            println!("\nPatches:");
            for p in &profiles {
                for patch in &p.patches {
                    println!("  {:<8} {:<22} → {} · {}", p.name, patch.name, patch.rig_preset, patch.snapshot);
                }
            }
            println!("\nPresets now: {}", comp.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "));
            if write {
                signal_guitar::library::RigLibrary::save_compositions(&comp);
                for p in &profiles {
                    signal_guitar::library::RigLibrary::save_profile(p);
                }
                println!("written.");
            } else {
                println!("(dry run — pass --write to save)");
            }
            ExitCode::SUCCESS
        }
        Command::DialPostComp {
            dry_run,
            sample_rate,
            threads,
            preset,
        } => {
            signal_guitar::levelling::apply_nam_calibration();
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            let started = std::time::Instant::now();
            let results = signal_guitar::compose::dial_post_comp(
                &mut comp,
                &lib.profile,
                &lib.drive_presets,
                sample_rate,
                threads,
                preset.as_deref(),
            );
            let mut failed = 0;
            let mut last = String::new();
            for r in &results {
                if r.preset != last {
                    println!("{}", r.preset);
                    last.clone_from(&r.preset);
                }
                match r.dialled {
                    Some((t, gr)) => println!(
                        "  {:<22} {:<13} threshold {t:>6.1} dB  GR {gr:>4.1} dB (target {:.1})",
                        r.snapshot, r.comp, r.target_gr_db
                    ),
                    None => {
                        failed += 1;
                        println!("  {:<22} did not render — left as it was", r.snapshot);
                    }
                }
            }
            if !dry_run {
                signal_guitar::library::RigLibrary::save_compositions(&comp);
            }
            println!(
                "\n{} snapshots in {:.0}s, {} failed{}",
                results.len(),
                started.elapsed().as_secs_f64(),
                failed,
                if dry_run { " (dry run — nothing written)" } else { "" }
            );
            if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Command::LevelPresets {
            dry_run,
            sample_rate,
            threads,
        } => {
            let cal = signal_guitar::levelling::apply_nam_calibration();
            println!(
                "NAM calibration: {}",
                cal.map_or("off".to_string(), |c| format!("{c} dBu interface"))
            );
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            if comp.presets.is_empty() {
                eprintln!("no presets loaded");
                return ExitCode::FAILURE;
            }
            let started = std::time::Instant::now();
            let results = signal_guitar::compose::level_presets(
                &mut comp,
                &lib.profile,
                &lib.drive_presets,
                sample_rate,
                threads,
            );
            let mut failed = 0;
            let mut last = String::new();
            for r in &results {
                if r.preset != last {
                    println!("{}", r.preset);
                    last.clone_from(&r.preset);
                }
                match r.lufs {
                    Some(l) => println!(
                        "  {:<20} {l:>7.1} LUFS  →  {:+.1} dB",
                        r.snapshot, r.level_db
                    ),
                    None => {
                        failed += 1;
                        println!(
                            "  {:<20} did not render — left at {:+.1} dB",
                            r.snapshot, r.level_db
                        );
                    }
                }
            }
            if !dry_run {
                signal_guitar::library::RigLibrary::save_compositions(&comp);
            }
            println!(
                "\n{} snapshots in {:.0}s, {} failed{}",
                results.len(),
                started.elapsed().as_secs_f64(),
                failed,
                if dry_run {
                    " (dry run — nothing written)"
                } else {
                    ""
                }
            );
            if failed > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Command::Freeze { preset, snapshot, epochs, play, check_only } => freeze(&preset, snapshot.as_deref(), epochs, play, check_only),
        Command::Frozen { preset, snapshot, state } => {
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            let Some(s) = signal_guitar::freeze::snapshot_mut(&mut comp, &preset, &snapshot) else {
                eprintln!("no Core snapshot {preset} · {snapshot}");
                return ExitCode::FAILURE;
            };
            if s.frozen_nam.is_empty() {
                eprintln!("{preset} · {snapshot} has not been frozen — `signal rig freeze` it first");
                return ExitCode::FAILURE;
            }
            s.frozen = matches!(state.as_str(), "on" | "frozen" | "true" | "1");
            println!("{preset} · {snapshot} plays {}", if s.frozen { "frozen" } else { "live" });
            signal_guitar::library::RigLibrary::save_compositions(&comp);
            ExitCode::SUCCESS
        }
        Command::Modules => {
            let comp = signal_guitar::library::RigLibrary::load_compositions();
            if comp.modules.is_empty() {
                eprintln!("no module presets loaded — modules.styx is missing or did not parse");
                return ExitCode::FAILURE;
            }
            let mut missing = 0;
            for m in &comp.modules {
                let snaps: Vec<&str> = m.snapshots.iter().map(|s| s.name.as_str()).collect();
                println!("{:<6} {:<24} {}", m.module, m.name, snaps.join(" · "));
                for s in &m.snapshots {
                    for p in [&s.nam, &s.cab, &s.nam2, &s.cab2] {
                        if !p.is_empty() && !std::path::Path::new(p).exists() {
                            missing += 1;
                            eprintln!("  missing: {p}");
                        }
                    }
                }
            }
            for b in &comp.blocks {
                println!(
                    "Block  {:<11} {:<24} {}",
                    b.block_type,
                    b.name,
                    if b.bypass { "(off)" } else { "" }
                );
            }
            let mut dangling = 0;
            let block_ok =
                |c: &signal_guitar::compose::BlockChoiceDef| comp.block_preset(&c.preset).is_some();
            for m in &comp.modules {
                for snap in &m.snapshots {
                    for c in snap.blocks.iter().filter(|c| !block_ok(c)) {
                        dangling += 1;
                        eprintln!(
                            "  {} {} / {}: no block preset {} for {}",
                            m.module, m.name, snap.name, c.preset, c.block
                        );
                    }
                }
            }
            for p in &comp.presets {
                for snap in &p.snapshots {
                    for c in snap.blocks.iter().filter(|c| !block_ok(c)) {
                        dangling += 1;
                        eprintln!(
                            "  {} / {}: no block preset {} for {}",
                            p.name, snap.name, c.preset, c.block
                        );
                    }
                }
            }
            for p in &comp.presets {
                for snap in &p.snapshots {
                    for pick in &snap.modules {
                        let ok = comp.module(&pick.module, &pick.preset).is_some_and(|m| {
                            pick.snapshot.is_empty()
                                || m.snapshots
                                    .iter()
                                    .any(|s| s.name.eq_ignore_ascii_case(&pick.snapshot))
                        });
                        if !ok {
                            dangling += 1;
                            eprintln!(
                                "  {} / {}: no {} preset {} / {}",
                                p.name, snap.name, pick.module, pick.preset, pick.snapshot
                            );
                        }
                    }
                }
            }
            println!(
                "\n{} block presets; {} module presets ({} files missing); {} presets, {} snapshots ({} picks dangling)",
                comp.blocks.len(),
                comp.modules.len(),
                missing,
                comp.presets.len(),
                comp.presets
                    .iter()
                    .map(|p| p.snapshots.len())
                    .sum::<usize>(),
                dangling
            );
            missing += dangling;
            if missing > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Command::Migrate { profile, write } => {
            match signal_guitar::compose::migrate_profile(&profile, !write) {
                Ok(m) => {
                    println!("Module presets:");
                    for module in &m.modules {
                        let snaps: Vec<&str> =
                            module.snapshots.iter().map(|s| s.name.as_str()).collect();
                        println!(
                            "  {:<6} {:<24} {}",
                            module.module,
                            module.name,
                            snaps.join(" · ")
                        );
                    }
                    println!("\nPresets:");
                    for preset in &m.presets {
                        println!("  {}", preset.name);
                        for snap in &preset.snapshots {
                            let picks: Vec<String> = snap
                                .modules
                                .iter()
                                .map(|p| format!("{}: {} / {}", p.module, p.preset, p.snapshot))
                                .collect();
                            println!(
                                "    {:<22} {}  (+{} overrides)",
                                snap.name,
                                picks.join(", "),
                                snap.overrides.len()
                            );
                        }
                    }
                    let left: Vec<&str> = m
                        .profile
                        .patches
                        .iter()
                        .filter(|p| p.rig_preset.is_empty())
                        .map(|p| p.name.as_str())
                        .collect();
                    if !left.is_empty() {
                        println!(
                            "\nLeft as they are (a second amp, or no pool capture): {}",
                            left.join(", ")
                        );
                    }
                    println!(
                        "{}",
                        if write {
                            "\nwritten."
                        } else {
                            "\n(dry run — pass --write to save)"
                        }
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Level {
            profile,
            dry_run,
            sample_rate,
        } => {
            match signal_guitar::levelling::level_profile(profile.as_deref(), sample_rate, dry_run)
            {
                Ok(results) => {
                    let mut failed = false;
                    for r in &results {
                        match r.lufs {
                            Some(lufs) => println!(
                                "{:<24} {lufs:>7.1} LUFS  →  level {:+.1} dB",
                                r.patch, r.level_db
                            ),
                            None => {
                                failed = true;
                                println!(
                                    "{:<24} did not render — level left at {:+.1} dB",
                                    r.patch, r.level_db
                                );
                            }
                        }
                    }
                    if dry_run {
                        println!("\n(dry run — nothing written)");
                    }
                    if failed {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

/// `signal rig reload`: drop a request into the rig directory, wait for the
/// running rig to answer it (it deletes the request), and print its report
/// lines from the reload log.
fn reload(timeout: u64) -> ExitCode {
    use signal_guitar::config_watch::{RELOAD_REQUEST, log_path};
    let dir = signal_guitar::library::rig_dir();
    let request = dir.join(RELOAD_REQUEST);
    let log = log_path();
    let from = std::fs::metadata(&log).map_or(0, |m| m.len());
    let tag = format!(
        "cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis())
    );
    if let Err(e) = std::fs::write(&request, &tag) {
        eprintln!("cannot write {}: {e}", request.display());
        return ExitCode::FAILURE;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    while request.exists() {
        if std::time::Instant::now() > deadline {
            let _ = std::fs::remove_file(&request);
            eprintln!(
                "no running rig answered within {timeout}s (watching {}). Is the app running on this config directory?",
                dir.display()
            );
            return ExitCode::FAILURE;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let text = std::fs::read(&log).unwrap_or_default();
    let new = String::from_utf8_lossy(text.get(from as usize..).unwrap_or_default());
    let marker = format!("[{tag}] ");
    for line in new.lines() {
        if let Some((_, rest)) = line.split_once(&marker) {
            println!("{rest}");
        }
    }
    ExitCode::SUCCESS
}

/// The NAM trainer script, written next to each freeze.
const TRAIN_SCRIPT: &str = include_str!("../../../features/rigs/guitar/scripts/nam_freeze_train.py");

fn freeze(preset: &str, snapshot: Option<&str>, epochs: u32, play: bool, check_only: bool) -> ExitCode {
    use signal_guitar::freeze;
    let cal = signal_guitar::levelling::apply_nam_calibration();
    println!("NAM calibration: {}", cal.map_or("off".to_string(), |c| format!("{c} dBu interface")));
    let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
    let mut comp = signal_guitar::library::RigLibrary::load_compositions();
    let Some(p) = comp.presets.iter().find(|p| p.name.eq_ignore_ascii_case(preset)) else {
        eprintln!("no Core preset {preset}");
        return ExitCode::FAILURE;
    };
    let preset = p.name.clone();
    let snaps: Vec<String> = p.snapshots.iter().map(|s| s.name.clone()).filter(|n| snapshot.is_none_or(|w| n.eq_ignore_ascii_case(w))).collect();
    if snaps.is_empty() {
        eprintln!("{preset} has no snapshot {}", snapshot.unwrap_or_default());
        return ExitCode::FAILURE;
    }
    let python = freeze::trainer_home().join("bin").join("python");
    if !check_only && (!python.exists() || !freeze::training_signal_path().exists()) {
        eprintln!(
            "the NAM trainer is not set up: want {} (a venv with neural-amp-modeler) and {} (the official v3 signal)",
            python.display(),
            freeze::training_signal_path().display()
        );
        return ExitCode::FAILURE;
    }
    let mut failed = 0;
    for snap in &snaps {
        let dir = freeze::freeze_dir(&preset, snap);
        println!("{preset} · {snap}  →  {}", dir.display());
        let (nam_l, nam_r) = if check_only {
            let Some(s) = freeze::snapshot_mut(&mut comp, &preset, snap).filter(|s| !s.frozen_nam.is_empty()) else {
                println!("  not frozen yet");
                failed += 1;
                continue;
            };
            (s.frozen_nam.clone(), if s.frozen_nam2.is_empty() { s.frozen_nam.clone() } else { s.frozen_nam2.clone() })
        } else {
            let t = std::time::Instant::now();
            let rendered = match freeze::render_training(&comp, &lib.profile, &lib.drive_presets, &preset, snap, &dir) {
                Ok(r) => r,
                Err(e) => {
                    println!("  did not render: {e}");
                    failed += 1;
                    continue;
                }
            };
            println!("  rendered in {:.0}s ({}, {:+.1} dB headroom)", t.elapsed().as_secs_f64(), if rendered.output_r.is_some() { "stereo" } else { "mono" }, rendered.headroom_db);
            let script = dir.join("train.py");
            if let Err(e) = std::fs::write(&script, TRAIN_SCRIPT) {
                println!("  {e}");
                failed += 1;
                continue;
            }
            let mut train = |side: &str, output: &std::path::Path| -> Option<String> {
                let t = std::time::Instant::now();
                let out = std::process::Command::new(&python)
                    .arg(&script)
                    .arg(freeze::training_signal_path())
                    .arg(output)
                    .arg(dir.join(format!("train-{side}")))
                    .arg(format!("core-{side}"))
                    .arg(epochs.to_string())
                    .arg(format!("{preset} · {snap} (frozen Core, {side})"))
                    .env("LD_LIBRARY_PATH", "/run/opengl-driver/lib")
                    .output()
                    .ok()?;
                let stdout = String::from_utf8_lossy(&out.stdout);
                let line = stdout.lines().rev().find(|l| l.starts_with('{'))?;
                let v: serde_json::Value = serde_json::from_str(line).ok()?;
                let Some(model) = v.get("model").and_then(|m| m.as_str()) else {
                    println!("  {side}: training failed\n{}", String::from_utf8_lossy(&out.stderr).lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"));
                    return None;
                };
                // Kept in the NAM library; the working copy stays in the cache.
                let kept = freeze::capture_path(&preset, snap, side);
                std::fs::create_dir_all(kept.parent()?).ok()?;
                std::fs::copy(model, &kept).ok()?;
                println!(
                    "  {side}: trained in {:.0} min, validation ESR {}",
                    t.elapsed().as_secs_f64() / 60.0,
                    v.get("esr").and_then(serde_json::Value::as_f64).map_or("?".into(), |e| format!("{e:.5}"))
                );
                Some(kept.to_string_lossy().into_owned())
            };
            let Some(l) = train("L", &rendered.output_l) else {
                failed += 1;
                continue;
            };
            let r = match &rendered.output_r {
                Some(o) => match train("R", o) {
                    Some(r) => r,
                    None => {
                        failed += 1;
                        continue;
                    }
                },
                None => l.clone(),
            };
            (l, r)
        };
        // The proof: a guitar the training never heard, live against frozen.
        match freeze::compare(&comp, &lib.profile, &lib.drive_presets, &preset, snap, &nam_l, &nam_r) {
            Ok((cl, cr)) => {
                for (side, c) in [("L", cl), ("R", cr)] {
                    println!("  {side}: frozen vs live  ESR {:.5}  level {:+.2} dB  lag {} samples", c.esr, c.trim_db, c.lag);
                }
                // One trim for both sides (they share it): the mean.
                let trim = (cl.trim_db + cr.trim_db) / 2.0;
                if let Some(s) = freeze::snapshot_mut(&mut comp, &preset, snap) {
                    s.frozen_nam = nam_l.clone();
                    s.frozen_nam2 = if nam_r == nam_l { String::new() } else { nam_r.clone() };
                    s.frozen_trim_db = trim;
                    s.frozen_esr = cl.esr.max(cr.esr);
                    if play {
                        s.frozen = true;
                    }
                }
                signal_guitar::library::RigLibrary::save_compositions(&comp);
                println!("  saved beside the live Core (trim {trim:+.2} dB){}", if play { " — playing frozen" } else { "" });
            }
            Err(e) => {
                println!("  did not compare: {e}");
                failed += 1;
            }
        }
    }
    if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

