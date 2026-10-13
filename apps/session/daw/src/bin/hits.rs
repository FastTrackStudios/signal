//! `hits <song>` — the hit detector on a song, headless: open it, find
//! every drum hit, write the hit markers (in memory; nothing is saved),
//! and say what was found and how long it took. With `--explain <piece>
//! <seconds>`, also what the detector saw there.
//!
//! The same code `HITS_DETECT` runs, without a window: for checking a
//! detector change against a real song.

use std::path::PathBuf;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(song) = args.first().map(PathBuf::from) else {
        eyre::bail!("usage: hits <song .RPP | .session> [--explain <piece> <seconds>]");
    };
    let explain = args.iter().position(|a| a == "--explain").and_then(|i| {
        Some((
            args.get(i + 1)?.clone(),
            args.get(i + 2)?.parse::<f64>().ok()?,
        ))
    });

    let started = std::time::Instant::now();
    let daw = daw::standalone::Standalone::new();
    session_daw::open::equip(&daw);
    let prepare = session_daw::prepare::Prepare::for_song(&song);
    let (opened, _) = session_daw::open_core::open_song_into(&daw, &song, &prepare)?;
    let project = daw::service::ProjectContext::Project(opened.project_guid.clone());
    println!("opened in {:.2} s", started.elapsed().as_secs_f64());

    let (pieces, extras) = session_daw::hits::kit(&daw, &project);
    for p in &pieces {
        println!("piece {}: trigger {}, mics {:?}", p.name, p.trigger, p.mics);
    }
    println!("extras {extras:?}");

    if args.iter().any(|a| a == "--takes") {
        for (origin, pass, used, files) in session_daw::hits::kit_takes(&daw, &project) {
            let names: Vec<String> = files
                .iter()
                .map(|(_, f)| {
                    std::path::Path::new(f)
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                })
                .collect();
            let used = if used { "in use" } else { "kept" };
            println!(
                "take at {origin:>9.3} s  pass {pass:<18} {used:<6} {} files: {names:?}",
                files.len()
            );
        }
        return Ok(());
    }
    let report = session_daw::hits::detect(
        &daw,
        &project,
        (f64::NEG_INFINITY, f64::INFINITY),
        &[],
        &session_daw::hits::HitSettings::default(),
        args.iter().any(|a| a == "--all-takes"),
    )
    .map_err(|e| eyre::eyre!(e))?;
    println!(
        "run {} over {} takes in {:.2} s (skipped: {:?})",
        report.run, report.takes, report.seconds, report.skipped
    );
    for p in &report.pieces {
        println!(
            "{:<8} {:>5} hits  both {:>4}  trigger only {:>3}  mic only {:>3}  ghosts {:>3}  flams {:>3}  unsure {:>3}  kept {:>2}  [{}]  {}  {}",
            p.piece,
            p.hits,
            p.both,
            p.trigger_only,
            p.mic_only,
            p.ghosts,
            p.flams,
            p.unsure,
            p.kept,
            p.method,
            p.gate,
            p.trigger
        );
    }

    let listed = session_daw::hits::hits(&daw, &project, &[]);
    if args.iter().any(|a| a == "--midi") {
        let track = session_daw::drum_reference::find_track(&daw, &project);
        println!("drum MIDI track: {track:?}");
        if let Some(track) = track {
            let notes = session_daw::drum_reference::notes(&daw, &project, &track);
            let mut pitches: std::collections::BTreeMap<u8, usize> =
                std::collections::BTreeMap::new();
            for n in &notes {
                *pitches.entry(n.pitch).or_default() += 1;
            }
            println!("{} notes by pitch: {pitches:?}", notes.len());
            let mut named: std::collections::BTreeMap<String, Vec<f64>> =
                std::collections::BTreeMap::new();
            for h in &listed {
                named.entry(h.piece.clone()).or_default().push(h.time);
            }
            let named: Vec<(String, Vec<f64>)> = named.into_iter().collect();
            for m in session_daw::drum_reference::learn_map(&notes, &named, 0.03) {
                println!("  {m:?}");
            }
            let t4: Vec<f64> = listed
                .iter()
                .filter(|h| h.piece == "Tom 4")
                .map(|h| h.time)
                .collect();
            println!(
                "Tom 4 hits at {:?}",
                t4.iter()
                    .map(|t| (t * 100.0).round() / 100.0)
                    .collect::<Vec<_>>()
            );
        }
    }
    println!("read back: {} hits", listed.len());
    for h in listed.iter().take(5) {
        println!("  {h:?}");
    }
    if let Some((piece, at)) = explain {
        let seen = session_daw::hits::explain(&daw, &project, &piece, at, (0.04, 40))
            .map_err(|e| eyre::eyre!(e))?;
        println!("{seen:#?}");
    }
    Ok(())
}
