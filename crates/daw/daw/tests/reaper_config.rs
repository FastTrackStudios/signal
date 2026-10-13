//! What gets versioned, and what must never be.

use daw::reaper_config::{self, filter_reaper_ini, merge_reaper_ini};
use std::path::Path;

#[test]
fn the_files_that_define_how_reaper_feels_are_tracked() {
    for f in [
        "reaper-kb.ini",
        "reaper-menu.ini",
        "reaper-mouse.ini",
        "reapack.ini",
        "ReaPack/registry.db",
    ] {
        assert!(
            reaper_config::is_tracked(Path::new(f)),
            "{f} should be tracked"
        );
    }
}

#[test]
fn reapacks_downloads_are_never_versioned() {
    // ReaPack owns everything it installs and registry.db restores it.
    // Committing the downloads would be vendoring other people's repos.
    for f in [
        "Scripts/ReaTeam Scripts/MIDI Editor/js_Mouse editing - Multi tool.lua",
        "Scripts/rtk/rtk.lua",
        "Scripts/cfillion/whatever.lua",
        "Effects/ReaTeam JSFX/thing.jsfx",
        "ReaPack/cache/something",
    ] {
        assert!(
            !reaper_config::is_tracked(Path::new(f)),
            "{f} must not be tracked"
        );
    }
    // Authors we have never heard of are excluded by default, because
    // the rule is an allowlist — a blocklist would have to enumerate
    // every author on ReaPack and would silently start vendoring the
    // next package you install.
    for f in [
        "Scripts/Cockos/thing.lua",
        "Scripts/sockmonkey72 Scripts/thing.lua",
        "Effects/sstillwell/thing.jsfx",
        "Scripts/some brand new author/thing.lua",
    ] {
        assert!(
            !reaper_config::is_tracked(Path::new(f)),
            "{f} must not be tracked"
        );
    }
    // Themes are deliberately outside the sweep: `ColorThemes/` holds
    // REAPER's stock themes and, if one has been unzipped, thousands of
    // images. Only the theme `reaper.ini` points at is exported, by
    // `referenced_themes` — so `is_tracked` says no here and that is
    // correct.
    assert!(!reaper_config::is_tracked(Path::new(
        "ColorThemes/Reapertips Theme.ReaperThemeZip"
    )));
    // But your own scripts are.
    assert!(reaper_config::is_tracked(Path::new(
        "Scripts/FTS/my_action.lua"
    )));
    assert!(reaper_config::is_tracked(Path::new(
        "Effects/FTS/mine.jsfx"
    )));
}

#[test]
fn binaries_state_and_backups_are_excluded() {
    for f in [
        "UserPlugins/reaper_sws.so", // binary, not config
        "Data/soundfont.sf2",        // ships with REAPER
        // REAPER's stock themes ship with every install.
        "ColorThemes/Default_7.0.ReaperThemeZip",
        "ColorThemes/Default_6.0.ReaperThemeZip",
        "reaper-menu.ini.bak",
        "fts-daw-reaper.log",
        "Scripts/thing.lua~",
    ] {
        assert!(
            !reaper_config::is_tracked(Path::new(f)),
            "{f} must not be tracked"
        );
    }
}

#[test]
fn filtering_drops_machine_state_and_keeps_preferences() {
    let ini = "\
[REAPER]
midieditor=1
defvzoom=4
audiodev=ALSA hw:2,0
wnd_x=100
wnd_y=250
recentfx_1=Pro-Q 3
lastproject=/home/me/song.rpp
autosaveint=5
[midi]
midieditor_flags=7
";
    let out = filter_reaper_ini(ini);
    // Preferences survive.
    assert!(out.contains("midieditor=1"));
    assert!(out.contains("defvzoom=4"));
    assert!(out.contains("autosaveint=5"));
    assert!(out.contains("midieditor_flags=7"));
    // One machine's hardware and layout must not follow to another.
    assert!(!out.contains("audiodev"), "audio device leaked");
    assert!(!out.contains("wnd_x"), "window geometry leaked");
    assert!(!out.contains("recentfx"), "session history leaked");
    assert!(!out.contains("lastproject"), "session history leaked");
    // Sections are kept so a later diff stays readable.
    assert!(out.contains("[REAPER]") && out.contains("[midi]"));
}

#[test]
fn merging_keeps_the_targets_hardware_and_takes_the_repos_preferences() {
    let existing = "\
[REAPER]
audiodev=ALSA hw:5,0
wnd_x=42
midieditor=0
";
    let incoming = "\
[REAPER]
midieditor=1
defvzoom=4
";
    let merged = merge_reaper_ini(existing, incoming);
    // Applying config must not reset this machine's sound card.
    assert!(
        merged.contains("audiodev=ALSA hw:5,0"),
        "machine keys must survive"
    );
    assert!(merged.contains("wnd_x=42"));
    // The repo wins on preferences, and the old value does not linger.
    assert!(merged.contains("midieditor=1"));
    assert!(!merged.contains("midieditor=0"), "stale preference kept");
    assert!(merged.contains("defvzoom=4"));
}

#[test]
fn export_then_apply_round_trips_through_a_temp_tree() {
    let tmp = std::env::temp_dir().join(format!("fts-rc-{}", std::process::id()));
    let live = tmp.join("live");
    let repo = tmp.join("repo");
    let restored = tmp.join("restored");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(live.join("Scripts/FTS")).unwrap();
    std::fs::create_dir_all(live.join("Scripts/SomeAuthor")).unwrap();
    std::fs::create_dir_all(live.join("ReaPack")).unwrap();

    std::fs::write(live.join("reaper-kb.ini"), "KEY 1 2 3").unwrap();
    std::fs::write(live.join("Scripts/FTS/mine.lua"), "-- mine").unwrap();
    std::fs::write(live.join("Scripts/SomeAuthor/theirs.lua"), "-- theirs").unwrap();
    std::fs::write(live.join("ReaPack/registry.db"), "sqlite").unwrap();
    std::fs::write(
        live.join("reaper.ini"),
        "[REAPER]\nmidieditor=1\naudiodev=hw:9\n",
    )
    .unwrap();

    let written = reaper_config::export(&live, &repo).unwrap();
    assert!(written.iter().any(|p| p.ends_with("mine.lua")));
    assert!(
        !written
            .iter()
            .any(|p| p.to_string_lossy().contains("SomeAuthor")),
        "ReaPack downloads must not be exported"
    );
    assert!(
        repo.join("ReaPack/registry.db").exists(),
        "the manifest is the point"
    );
    let stored = std::fs::read_to_string(repo.join("reaper.ini")).unwrap();
    assert!(!stored.contains("audiodev"));

    std::fs::create_dir_all(&restored).unwrap();
    std::fs::write(
        restored.join("reaper.ini"),
        "[REAPER]\naudiodev=hw:0\nmidieditor=0\n",
    )
    .unwrap();
    reaper_config::apply(&repo, &restored).unwrap();

    assert_eq!(
        std::fs::read_to_string(restored.join("Scripts/FTS/mine.lua")).unwrap(),
        "-- mine"
    );
    let merged = std::fs::read_to_string(restored.join("reaper.ini")).unwrap();
    assert!(merged.contains("audiodev=hw:0"), "target hardware survived");
    assert!(merged.contains("midieditor=1"), "repo preference applied");

    // A freshly exported tree has nothing to report.
    assert!(reaper_config::diff(&live, &repo).is_empty());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn the_theme_in_use_travels_but_its_absolute_path_does_not() {
    let ini =
        "[REAPER]\nlastthemefn5=/home/cody/fts-dev/ColorThemes/Reapertips Theme.ReaperThemeZip\n";
    // The choice of theme is a preference and must follow.
    assert!(filter_reaper_ini(ini).contains("lastthemefn5"));
    // And we know which file to carry.
    let themes = reaper_config::referenced_themes(ini);
    assert_eq!(
        themes,
        vec![Path::new("ColorThemes/Reapertips Theme.ReaperThemeZip").to_path_buf()]
    );

    // The absolute path is true of one machine only, so it is
    // tokenised out and restored per-machine on apply.
    let live = Path::new("/home/cody/fts-dev");
    let stored = reaper_config::tokenise_paths(ini, live);
    assert!(
        !stored.contains("/home/cody"),
        "absolute path leaked: {stored}"
    );
    assert!(stored.contains(reaper_config::RESOURCES_TOKEN));

    let elsewhere = Path::new("/Users/someone/Library/REAPER");
    let restored = reaper_config::expand_paths(&stored, elsewhere);
    assert!(restored.contains("/Users/someone/Library/REAPER/ColorThemes/Reapertips"));
    assert!(!restored.contains(reaper_config::RESOURCES_TOKEN));
}

#[test]
fn only_the_referenced_theme_is_exported_and_it_comes_back() {
    let tmp = std::env::temp_dir().join(format!("fts-rc-theme-{}", std::process::id()));
    let live = tmp.join("live");
    let repo = tmp.join("repo");
    let restored = tmp.join("restored");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(live.join("ColorThemes")).unwrap();

    std::fs::write(live.join("ColorThemes/Mine.ReaperThemeZip"), "mine").unwrap();
    // Stock themes ship with REAPER; an unzipped theme is thousands of
    // images. Neither should travel.
    std::fs::write(live.join("ColorThemes/Default_7.0.ReaperThemeZip"), "stock").unwrap();
    std::fs::write(live.join("ColorThemes/Unused.ReaperThemeZip"), "unused").unwrap();
    std::fs::write(
        live.join("reaper.ini"),
        format!(
            "[REAPER]\nlastthemefn5={}/ColorThemes/Mine.ReaperThemeZip\n",
            live.display()
        ),
    )
    .unwrap();

    reaper_config::export(&live, &repo).unwrap();
    assert!(repo.join("ColorThemes/Mine.ReaperThemeZip").exists());
    assert!(!repo.join("ColorThemes/Default_7.0.ReaperThemeZip").exists());
    assert!(
        !repo.join("ColorThemes/Unused.ReaperThemeZip").exists(),
        "only the theme in use travels"
    );

    // The path must be rewritten for the machine it lands on.
    std::fs::create_dir_all(&restored).unwrap();
    reaper_config::apply(&repo, &restored).unwrap();
    assert!(restored.join("ColorThemes/Mine.ReaperThemeZip").exists());
    let ini = std::fs::read_to_string(restored.join("reaper.ini")).unwrap();
    assert!(
        ini.contains(&format!("{}/ColorThemes/Mine", restored.display())),
        "theme path must point at this machine, got: {ini}"
    );
    assert!(!ini.contains("$REAPER_RESOURCES"));

    let _ = std::fs::remove_dir_all(&tmp);
}
