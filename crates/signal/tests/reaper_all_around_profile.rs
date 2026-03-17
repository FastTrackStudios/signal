//! REAPER integration test: Build the "All-Around" profile with 8 track template variations.
//!
//! Each patch variant (Clean, Crunch, Drive, Lead, Funk, Ambient, Q-Tron, Solo)
//! is saved as its own `.RTrackTemplate` variation under the profile preset folder.
//!
//! Layout:
//! ```text
//! TrackTemplates/FTS-Signal/Guitar/04-Profiles/All-Around/
//! ├── Clean.RTrackTemplate     + Clean.signal.styx
//! ├── Crunch.RTrackTemplate    + Crunch.signal.styx
//! ├── Drive.RTrackTemplate     ...
//! └── Solo.RTrackTemplate
//! ```
//!
//! Run with:
//!   cargo xtask reaper-test all_around_profile

use std::time::Duration;

use reaper_test::reaper_test;
use signal::sidecar;
use signal::track_template::{self, Instrument, TemplateTier};
use signal::{ModuleRepo, ModuleRepoLive};

async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

/// Each variant loads a different subset of module types.
const VARIANTS: &[(&str, &[&str])] = &[
    ("Clean",   &["input", "amp", "master"]),
    ("Crunch",  &["input", "drive", "amp", "master"]),
    ("Drive",   &["input", "drive", "amp", "dynamics", "master"]),
    ("Lead",    &["input", "drive", "amp", "modulation", "time", "master"]),
    ("Funk",    &["input", "amp", "dynamics", "modulation", "master"]),
    ("Ambient", &["input", "amp", "modulation", "time", "master"]),
    ("Q-Tron",  &["input", "drive", "amp", "modulation", "master"]),
    ("Solo",    &["input", "drive", "amp", "time", "dynamics", "master"]),
];

// ---------------------------------------------------------------------------
// Build all 8 variants, save each as a track template variation
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn all_around_profile(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Connect to the real signal.db
    let db_path = utils::paths::signal_db().to_string_lossy().to_string();
    let signal = signal::connect_db_seeded(&db_path).await?;
    let svc = signal.service();

    // Get all modules from the DB
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

    ctx.log(&format!(
        "Available module types: {:?}",
        module_by_type.keys().collect::<Vec<_>>()
    ));

    // ── Build and save each variant as a track template variation ──

    let profile_name = "All-Around";
    let mut saved = Vec::new();

    for &(variant_name, module_types) in VARIANTS {
        ctx.log(&format!("\n── {variant_name} ──"));

        // Clean slate
        let _ = project.tracks().remove_all().await;
        settle().await;

        let track = project
            .tracks()
            .add(&format!("[L] {variant_name}"), None)
            .await?;
        settle().await;

        let mut loaded = 0usize;
        for &type_key in module_types {
            let Some(module_preset) = module_by_type.get(type_key) else {
                ctx.log(&format!("  SKIP {type_key} — no module found"));
                continue;
            };

            let module_type = module_preset.module_type();
            let module_id = module_preset.id().clone();
            let name = module_preset.name().to_string();

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

        // Capture the full track chunk (not just FX chain — profiles are track templates)
        let chunk = track.get_chunk().await?;

        // Save as a variation of the profile preset
        // e.g. TrackTemplates/FTS-Signal/Guitar/04-Profiles/All-Around/Clean.RTrackTemplate
        let path = track_template::save_track_template(
            profile_name,
            variant_name,
            Instrument::Guitar,
            TemplateTier::Profile,
            &chunk,
            &uuid::Uuid::new_v4().to_string(),
            &[
                "guitar".to_string(),
                "all-around".to_string(),
                variant_name.to_lowercase(),
            ],
            Some(&format!(
                "{variant_name} — {} modules, {} FX",
                loaded, fx_count
            )),
        )?;

        ctx.log(&format!(
            "  Saved: {} ({} bytes, {} modules, {} FX)",
            path.display(),
            chunk.len(),
            loaded,
            fx_count,
        ));
        saved.push((variant_name, path));
    }

    // ── Summary ──

    ctx.log(&format!(
        "\n── All-Around profile: {} variations saved ──",
        saved.len()
    ));
    for (name, path) in &saved {
        let styx = sidecar::sidecar_path(path);
        ctx.log(&format!(
            "  {name}: template={} styx={}",
            path.exists(),
            styx.exists()
        ));
    }

    // Verify all files exist
    assert_eq!(saved.len(), VARIANTS.len(), "all variants should be saved");
    for (name, path) in &saved {
        assert!(path.exists(), "{name}.RTrackTemplate should exist");

        let styx = sidecar::sidecar_path(path);
        assert!(styx.exists(), "{name}.signal.styx should exist");

        // Verify content is a valid track chunk
        let content = std::fs::read_to_string(path)?;
        assert!(
            content.contains("<TRACK"),
            "{name}.RTrackTemplate should contain <TRACK block"
        );
        assert!(
            content.contains("<FXCHAIN"),
            "{name}.RTrackTemplate should contain FXCHAIN block"
        );
    }

    // Verify scanner finds the profile variations
    let root = track_template::track_templates_root();
    let scanned = track_template::scan_track_templates(&root);
    let profile_variants: Vec<_> = scanned
        .iter()
        .filter(|t| t.preset_name == profile_name && t.tier == Some(TemplateTier::Profile))
        .collect();
    ctx.log(&format!(
        "\nScanner found {} All-Around profile variations",
        profile_variants.len()
    ));
    for t in &profile_variants {
        ctx.log(&format!("  {} / {}", t.preset_name, t.variation_name));
    }
    assert_eq!(
        profile_variants.len(),
        VARIANTS.len(),
        "scanner should find all 8 profile variations"
    );

    ctx.log("\nall_around_profile: PASS");
    Ok(())
}
