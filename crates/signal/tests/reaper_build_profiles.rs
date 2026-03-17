//! REAPER integration test: Build Worship and Rock profiles with track template variations.
//!
//! Each profile gets a set of named variations saved as individual
//! `.RTrackTemplate` files under `TrackTemplates/FTS-Signal/Guitar/04-Profiles/`.
//!
//! Run with:
//!   cargo xtask reaper-test build_profiles

use std::time::Duration;

use reaper_test::reaper_test;
use signal::track_template::{self, Instrument, TemplateTier};
use signal::{ModuleRepo, ModuleRepoLive};

async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

async fn long_settle() {
    tokio::time::sleep(Duration::from_millis(2000)).await;
}

async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

/// A profile definition: name + list of (variation_name, module_types).
struct ProfileDef {
    name: &'static str,
    tag: &'static str,
    description: &'static str,
    variants: &'static [(&'static str, &'static [&'static str])],
}

const WORSHIP: ProfileDef = ProfileDef {
    name: "Worship",
    tag: "worship",
    description: "Worship guitar tones — clean, atmospheric, ambient",
    variants: &[
        ("Clean",    &["input", "amp", "master"]),
        ("Shimmer",  &["input", "amp", "modulation", "time", "master"]),
        ("Pad",      &["input", "amp", "modulation", "time", "master"]),
        ("Swell",    &["input", "amp", "dynamics", "time", "master"]),
        ("Ambient",  &["input", "amp", "modulation", "time", "master"]),
        ("Sparkle",  &["input", "amp", "modulation", "master"]),
        ("Lead",     &["input", "drive", "amp", "time", "master"]),
        ("Edge",     &["input", "drive", "amp", "modulation", "master"]),
    ],
};

const ROCK: ProfileDef = ProfileDef {
    name: "Rock",
    tag: "rock",
    description: "Rock guitar tones — crunch, drive, power",
    variants: &[
        ("Clean",       &["input", "amp", "master"]),
        ("Crunch",      &["input", "drive", "amp", "master"]),
        ("Rhythm",      &["input", "drive", "amp", "dynamics", "master"]),
        ("Power",       &["input", "drive", "amp", "dynamics", "master"]),
        ("Lead",        &["input", "drive", "amp", "modulation", "time", "master"]),
        ("Solo",        &["input", "drive", "amp", "time", "dynamics", "master"]),
        ("Dirty Clean", &["input", "drive", "amp", "master"]),
        ("Boost",       &["input", "drive", "amp", "dynamics", "master"]),
    ],
};

// ---------------------------------------------------------------------------
// Build all profiles
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn build_profiles(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Connect to signal.db
    let db_path = utils::paths::signal_db().to_string_lossy().to_string();
    let signal = signal::connect_db_seeded(&db_path).await?;
    let svc = signal.service();

    let db_url = format!("sqlite:{db_path}?mode=ro");
    let db = signal::Database::connect(&db_url).await?;
    let module_repo = ModuleRepoLive::new(db);
    module_repo.init_schema().await?;
    let all_modules = module_repo.list_module_collections().await?;

    ctx.log(&format!("Found {} module presets in DB", all_modules.len()));

    // Build lookup: module_type → first module preset of that type
    let mut module_by_type = std::collections::HashMap::new();
    for module in &all_modules {
        let type_key = module.module_type().as_str().to_string();
        module_by_type.entry(type_key).or_insert(module);
    }

    // Build each profile with a pause between them
    let profiles: &[&ProfileDef] = &[&WORSHIP, &ROCK];
    for (profile_idx, profile) in profiles.iter().enumerate() {
        if profile_idx > 0 {
            // Extra pause between profiles to let REAPER fully settle
            long_settle().await;
        }
        let sep = "=".repeat(60);
        ctx.log(&format!(
            "\n{sep}\n  Profile: {} — {}\n{sep}",
            profile.name, profile.description
        ));

        let mut saved = 0usize;

        for &(variant_name, module_types) in profile.variants {
            ctx.log(&format!("\n── {} / {} ──", profile.name, variant_name));

            let _ = project.tracks().remove_all().await;
            // Brief pause between variants
            tokio::time::sleep(Duration::from_millis(300)).await;

            let track = project
                .tracks()
                .add(&format!("[L] {}", variant_name), None)
                .await?;
            settle().await;

            let mut loaded = 0usize;
            for &type_key in module_types {
                let Some(mp) = module_by_type.get(type_key) else {
                    ctx.log(&format!("  SKIP {type_key} — no module found"));
                    continue;
                };

                let module_type = mp.module_type();
                let module_id = mp.id().clone();
                let name = mp.name().to_string();

                match svc
                    .load_module_to_track(module_type, &module_id, 0, &track)
                    .await
                {
                    Ok(result) => {
                        settle().await;
                        ctx.log(&format!(
                            "  [{type_key}] {name} — {} FX",
                            result.loaded_fx.len()
                        ));
                        loaded += 1;
                    }
                    Err(e) => {
                        ctx.log(&format!("  FAIL [{type_key}] {name} — {e}"));
                    }
                }
            }

            let fx_count = track.fx_chain().count().await?;
            if fx_count == 0 {
                ctx.log(&format!("  SKIP {variant_name} (no FX loaded)"));
                continue;
            }

            let chunk = track.get_chunk().await?;
            let path = track_template::save_track_template(
                profile.name,
                variant_name,
                Instrument::Guitar,
                TemplateTier::Profile,
                &chunk,
                &uuid::Uuid::new_v4().to_string(),
                &[
                    "guitar".to_string(),
                    profile.tag.to_string(),
                    variant_name.to_lowercase(),
                ],
                Some(&format!(
                    "{} / {} — {} modules, {} FX",
                    profile.name, variant_name, loaded, fx_count
                )),
            )?;

            ctx.log(&format!(
                "  Saved: {} ({} bytes, {} modules, {} FX)",
                path.display(),
                chunk.len(),
                loaded,
                fx_count,
            ));
            saved += 1;
        }

        ctx.log(&format!(
            "\n  {} profile: {}/{} variations saved",
            profile.name,
            saved,
            profile.variants.len()
        ));
        assert!(saved > 0, "{} should save at least 1 variation", profile.name);
    }

    // ── Verify scanner finds everything ──

    let root = track_template::track_templates_root();
    let scanned = track_template::scan_track_templates(&root);

    for profile in &[WORSHIP, ROCK] {
        let variants: Vec<_> = scanned
            .iter()
            .filter(|t| t.preset_name == profile.name && t.tier == Some(TemplateTier::Profile))
            .collect();
        ctx.log(&format!(
            "\nScanner: {} has {} variations",
            profile.name,
            variants.len()
        ));
        for t in &variants {
            ctx.log(&format!("  {} / {}", t.preset_name, t.variation_name));
        }
        assert_eq!(
            variants.len(),
            profile.variants.len(),
            "{} should have {} variations",
            profile.name,
            profile.variants.len()
        );
    }

    ctx.log("\nbuild_profiles: PASS");
    Ok(())
}
