//! Reaper tag hierarchy — the complete taxonomy for Reaper concepts.

use architect_launcher_core::TagRegistry;

/// Register all Reaper-specific tags with colors.
pub fn register_reaper_tags(tags: &mut TagRegistry) {
    // ── Audio (green family) ───────────────────────────────
    tags.register("audio", "Audio", "Audio production")
        .set_color("audio", "#a6e3a1");

    // Effects
    tags.register("audio/effects", "Effects", "Audio effects")
        .set_color("audio/effects", "#a6e3a1")
        .register("audio/effects/reverb", "Reverb", "Reverb effects")
        .register("audio/effects/delay", "Delay", "Delay effects")
        .register("audio/effects/eq", "EQ", "Equalizers")
        .register(
            "audio/effects/dynamics",
            "Dynamics",
            "Compressors, gates, limiters",
        )
        .register(
            "audio/effects/distortion",
            "Distortion",
            "Distortion and saturation",
        )
        .set_color("audio/effects/distortion", "#f38ba8")
        .register(
            "audio/effects/modulation",
            "Modulation",
            "Chorus, flanger, phaser",
        )
        .set_color("audio/effects/modulation", "#74c7ec")
        .register("audio/effects/filter", "Filter", "Filter effects")
        .register("audio/effects/stereo", "Stereo", "Stereo width and imaging")
        .register(
            "audio/effects/pitch",
            "Pitch",
            "Pitch shifting and correction",
        )
        .register(
            "audio/effects/utility",
            "Utility",
            "Utility effects (gain, phase, etc.)",
        );

    // Plugin formats
    tags.register("audio/effects/plugin", "Plugins", "Plugin formats")
        .register("audio/effects/plugin/vst", "VST", "VST2 plugins")
        .register("audio/effects/plugin/vst3", "VST3", "VST3 plugins")
        .register("audio/effects/plugin/clap", "CLAP", "CLAP plugins")
        .register("audio/effects/plugin/jsfx", "JSFX", "Jesusonic effects")
        .register("audio/effects/plugin/lv2", "LV2", "LV2 plugins");

    // Instruments
    tags.register("audio/instruments", "Instruments", "Virtual instruments")
        .set_color("audio/instruments", "#94e2d5")
        .register("audio/instruments/synth", "Synth", "Synthesizers")
        .register("audio/instruments/sampler", "Sampler", "Samplers")
        .register("audio/instruments/piano", "Piano", "Piano instruments")
        .register("audio/instruments/drums", "Drums", "Drum machines")
        .register("audio/instruments/organ", "Organ", "Organ instruments")
        .register("audio/instruments/strings", "Strings", "String instruments")
        .register("audio/instruments/bass", "Bass", "Bass instruments");

    // ── Reaper (peach/orange family) ───────────────────────
    tags.register("reaper", "Reaper", "Reaper DAW")
        .set_color("reaper", "#fab387");

    // Actions
    tags.register("reaper/actions", "Actions", "Reaper actions")
        .set_color("reaper/actions", "#fab387")
        .register("reaper/actions/file", "File", "File operations")
        .register("reaper/actions/edit", "Edit", "Edit operations")
        .register(
            "reaper/actions/transport",
            "Transport",
            "Transport controls",
        )
        .register("reaper/actions/view", "View", "View/UI actions")
        .register("reaper/actions/insert", "Insert", "Insert operations")
        .register("reaper/actions/item", "Item", "Media item actions")
        .register("reaper/actions/track", "Track", "Track actions")
        .register("reaper/actions/options", "Options", "Options/preferences")
        .register(
            "reaper/actions/extensions",
            "Extensions",
            "Extension actions",
        )
        .register("reaper/actions/scripts", "Scripts", "ReaScript actions");

    // Tracks
    tags.register("reaper/tracks", "Tracks", "Reaper tracks")
        .set_color("reaper/tracks", "#f5c2e7");

    // FX chains
    tags.register("reaper/fx-chains", "FX Chains", "Saved FX chain presets")
        .set_color("reaper/fx-chains", "#89b4fa");

    // Items
    tags.register("reaper/items", "Items", "Media items")
        .set_color("reaper/items", "#f9e2af");

    // Markers & regions
    tags.register("reaper/markers", "Markers", "Markers and regions")
        .set_color("reaper/markers", "#cba6f7")
        .register("reaper/markers/marker", "Marker", "Project markers")
        .register("reaper/markers/region", "Region", "Project regions");

    // Templates
    tags.register(
        "reaper/templates",
        "Templates",
        "Project and track templates",
    )
    .register(
        "reaper/templates/project",
        "Project Templates",
        "Project templates",
    )
    .register(
        "reaper/templates/track",
        "Track Templates",
        "Track templates",
    );

    // Visibility
    tags.register(
        "reaper/visibility",
        "Visibility",
        "Track visibility controls",
    )
    .set_color("reaper/visibility", "#cba6f7");

    // ── Desktop (blue) ─────────────────────────────────────
    tags.register("desktop", "Desktop", "Desktop environment")
        .set_color("desktop", "#89b4fa")
        .register(
            "desktop/applications",
            "Applications",
            "Desktop applications",
        );

    // ── Tools (yellow) ─────────────────────────────────────
    tags.register("tools", "Tools", "Utility tools")
        .set_color("tools", "#f9e2af")
        .register("tools/calculator", "Calculator", "Math calculator");

    // ── Aliases ────────────────────────────────────────────
    tags.alias("apps", "desktop/applications")
        .alias("fx", "audio/effects")
        .alias("vst", "audio/effects/plugin/vst")
        .alias("vst3", "audio/effects/plugin/vst3")
        .alias("clap", "audio/effects/plugin/clap")
        .alias("jsfx", "audio/effects/plugin/jsfx")
        .alias("inst", "audio/instruments")
        .alias("act", "reaper/actions")
        .alias("calc", "tools/calculator")
        .alias("vis", "reaper/visibility");
}
