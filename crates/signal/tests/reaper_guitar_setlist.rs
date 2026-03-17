//! REAPER integration test: Batch-load a full guitar setlist from profile templates.
//!
//! Reads all needed .RTrackTemplate files upfront, then creates all tracks
//! back-to-back with no settle() delays — as fast as the DAW API allows.
//!
//! Run with:
//!   cargo xtask reaper-test guitar_setlist

use std::time::Duration;

use reaper_test::reaper_test;
use signal::track_template;

async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

// ─── Setlist definition ──────────────────────────────────────

struct SongDef {
    title: &'static str,
    artist: &'static str,
    profile: &'static str,
    sections: &'static [(&'static str, &'static str)],
}

const SETLIST: &[SongDef] = &[
    SongDef {
        title: "Belief", artist: "John Mayer", profile: "All-Around",
        sections: &[("Intro", "Clean"), ("Verse", "Clean"), ("Chorus", "Crunch"),
                     ("Solo", "Lead"), ("Outro", "Clean")],
    },
    SongDef {
        title: "Vienna", artist: "Couch", profile: "Worship",
        sections: &[("Intro", "Shimmer"), ("Verse", "Clean"), ("Chorus", "Pad"),
                     ("Bridge", "Ambient"), ("Outro", "Shimmer")],
    },
    SongDef {
        title: "Anomalie Medley", artist: "Anomalie", profile: "All-Around",
        sections: &[("Intro", "Funk"), ("A", "Funk"), ("B", "Clean"),
                     ("Solo", "Lead"), ("Outro", "Funk")],
    },
    SongDef {
        title: "New Whip", artist: "Eliv8", profile: "All-Around",
        sections: &[("Intro", "Clean"), ("Verse", "Funk"),
                     ("Chorus", "Crunch"), ("Drop", "Drive")],
    },
    SongDef {
        title: "Leave No Stone", artist: "Intervals", profile: "Rock",
        sections: &[("Intro", "Clean"), ("Riff", "Power"), ("Lead", "Lead"),
                     ("Breakdown", "Rhythm"), ("Solo", "Solo")],
    },
    SongDef {
        title: "Won't Stand Down", artist: "Muse", profile: "Rock",
        sections: &[("Intro", "Crunch"), ("Verse", "Rhythm"), ("Chorus", "Power"),
                     ("Bridge", "Clean"), ("Solo", "Lead")],
    },
    SongDef {
        title: "What About Me", artist: "Snarky Puppy", profile: "All-Around",
        sections: &[("Intro", "Clean"), ("Groove", "Funk"), ("Solo", "Lead"),
                     ("Build", "Crunch"), ("Outro", "Q-Tron")],
    },
    SongDef {
        title: "Harder to Breathe", artist: "Maroon 5", profile: "Rock",
        sections: &[("Intro", "Crunch"), ("Verse", "Crunch"), ("Chorus", "Power"),
                     ("Solo", "Lead"), ("Outro", "Power")],
    },
];

// ─── Batch-load setlist ──────────────────────────────────────

/// Build the list of (track_name, chunk) entries for the entire setlist.
fn build_track_list(
    setlist: &[SongDef],
    cache: &std::collections::HashMap<(String, String), String>,
) -> Vec<(String, String)> {
    let mut entries = Vec::new();

    for song in setlist {
        // Deduplicate variations
        let mut song_vars: Vec<&str> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for &(_, variation) in song.sections {
            if seen.insert(variation) {
                song_vars.push(variation);
            }
        }

        // Song folder track (minimal chunk with ISBUS 1 1)
        let folder_name = format!("[S] {} — {}", song.title, song.artist);
        entries.push((
            folder_name.clone(),
            format!("<TRACK\nNAME \"{folder_name}\"\nISBUS 1 1\n>"),
        ));

        // Variation tracks
        let var_count = song_vars.len();
        for (i, variation) in song_vars.iter().enumerate() {
            let key = (song.profile.to_string(), variation.to_string());
            if let Some(chunk) = cache.get(&key) {
                let final_chunk = if i == var_count - 1 {
                    set_isbus_in_chunk(chunk, -1)
                } else {
                    // Remove any ISBUS from template (child tracks should be depth 0)
                    set_isbus_in_chunk(chunk, 0)
                };
                entries.push((format!("[L] {variation}"), final_chunk));
            }
        }
    }

    entries
}

#[reaper_test(isolated)]
async fn guitar_setlist(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    let root = track_template::track_templates_root();

    // Collect unique variations needed
    let mut needed: std::collections::BTreeSet<(&str, &str)> = std::collections::BTreeSet::new();
    for song in SETLIST {
        for &(_, variation) in song.sections {
            needed.insert((song.profile, variation));
        }
    }

    // Pre-load all template chunks from disk
    let mut cache: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    for &(profile, variation) in &needed {
        let path = root
            .join("Guitar/04-Profiles")
            .join(profile)
            .join(format!("{variation}.RTrackTemplate"));
        if let Ok(chunk) = std::fs::read_to_string(&path) {
            cache.insert((profile.to_string(), variation.to_string()), chunk);
        } else {
            ctx.log(&format!("MISSING: {profile}/{variation}"));
        }
    }
    ctx.log(&format!("Cached {}/{} templates", cache.len(), needed.len()));

    // Build the full track list
    let entries = build_track_list(SETLIST, &cache);
    ctx.log(&format!("Prepared {} tracks", entries.len()));

    // ── Batch-create all tracks (no sleeps between) ──

    let start = std::time::Instant::now();
    let mut created = 0usize;

    for (name, chunk) in &entries {
        let track = project.tracks().add(name, None).await?;
        // set_chunk immediately — no settle needed, the daw service dispatches
        // to main thread synchronously
        if let Err(e) = track.set_chunk(chunk.clone()).await {
            ctx.log(&format!("  ✗ {name}: {:?}", e));
        } else {
            created += 1;
        }
    }

    let elapsed = start.elapsed();
    ctx.log(&format!(
        "Created {} tracks in {:.1}s ({:.0}ms per track)",
        created,
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / created as f64,
    ));

    // Give REAPER a moment to finish plugin instantiation
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // ── Summary ──

    let all = project.tracks().all().await?;
    ctx.log(&format!("Project: {} tracks", all.len()));

    for song in SETLIST {
        let mut vars: Vec<&str> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for &(_, v) in song.sections {
            if seen.insert(v) { vars.push(v); }
        }
        ctx.log(&format!(
            "  {} [{}]: {}",
            song.title, song.profile, vars.join(", ")
        ));
    }

    assert_eq!(created, entries.len(), "all tracks should be created");

    ctx.log("\nguitar_setlist: PASS");
    Ok(())
}

/// Patch the ISBUS line in a track chunk.
fn set_isbus_in_chunk(chunk: &str, depth: i32) -> String {
    let isbus_line = if depth > 0 {
        format!("ISBUS 1 {depth}")
    } else if depth < 0 {
        format!("ISBUS 2 {depth}")
    } else {
        // depth 0 = remove ISBUS
        return chunk
            .lines()
            .filter(|l| !l.trim_start().starts_with("ISBUS "))
            .collect::<Vec<_>>()
            .join("\n");
    };

    let has_isbus = chunk.lines().any(|l| l.trim_start().starts_with("ISBUS "));
    if has_isbus {
        chunk
            .lines()
            .map(|l| if l.trim_start().starts_with("ISBUS ") { isbus_line.as_str() } else { l })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let mut result = Vec::new();
        let mut inserted = false;
        for line in chunk.lines() {
            result.push(line);
            if !inserted && line.trim_start().starts_with("NAME ") {
                result.push(&isbus_line);
                inserted = true;
            }
        }
        if !inserted && !result.is_empty() {
            result.insert(1, &isbus_line);
        }
        result.join("\n")
    }
}
