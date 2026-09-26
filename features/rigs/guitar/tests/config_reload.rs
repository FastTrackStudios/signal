//! Hot reload end to end on a design-mode rig over a throwaway library that
//! saves: a styx file edited under the running rig is applied live, the
//! rig's own saves never undo it, and a file that does not parse changes
//! nothing and is never written over.
//!
//! One test, one backend: the library reads its directory from the process
//! environment, and a second backend's meter pump would watch the same one.

use std::path::Path;
use std::time::{Duration, Instant};

use signal_guitar::GuitarRigBackend;
use signal_guitar::library::{SetlistLib, SongLib};
use signal_guitar::proto::rig::Rig;
use signal_guitar::profiles::ProfileDef;

fn read<T: for<'a> facet::Facet<'a>>(path: &Path) -> T {
    facet_styx::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn write<T: for<'a> facet::Facet<'a>>(path: &Path, value: &T) {
    std::fs::write(path, facet_styx::to_string(value).unwrap()).unwrap();
}

/// Wait (on the file watcher) until `ok` holds.
fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ok() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait until the reload log has a line containing `text` (a reload is
/// logged once it is fully applied).
fn logged(log: &Path, text: &str) {
    eventually(&format!("the reload log to say {text:?}"), || {
        std::fs::read_to_string(log).is_ok_and(|l| l.contains(text))
    });
}

fn library_bpm(rig: &GuitarRigBackend, song: &str) -> Option<u32> {
    Rig::perf(rig).library_songs.into_iter().find(|s| s.name == song).map(|s| s.bpm)
}

#[test]
fn a_config_file_edited_under_the_running_rig_applies_live_and_sticks() {
    let root = std::env::temp_dir().join(format!("config-reload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let log = root.join("config-reload.log");
    // SAFETY: the only test in this binary; nothing else reads the env yet.
    unsafe {
        std::env::set_var("SIGNAL_RIG_DIR", root.join("rig"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
        std::env::set_var("SIGNAL_RELOAD_LOG", &log);
    }
    let dir = root.join("rig");
    let songs_path = dir.join("songs.styx");
    let sets_path = dir.join("setlists.styx");

    let rig = GuitarRigBackend::new();
    rig.open_blocking();
    assert_eq!(
        Rig::reload_config(&rig),
        "nothing to reload: every config file is as the rig has it"
    );

    // On the second song of the first set, at its third part.
    Rig::set_perform_mode(&rig, 2);
    Rig::select_song(&rig, 1);
    Rig::select_part(&rig, 2);
    let perf = Rig::perf(&rig);
    assert_eq!(perf.songs[1].name, "No Other Name");
    assert_eq!((perf.song_index, perf.part_index), (1, 2));

    // ── songs.styx edited by hand: the watcher applies it, no restart ──
    let mut lib: SongLib = read(&songs_path);
    let song = lib.songs.iter_mut().find(|s| s.name == "No Other Name").unwrap();
    song.bpm = 91;
    song.parts.push("Tag".into());
    write(&songs_path, &lib);
    logged(&log, "songs.styx: 10 songs; on No Other Name / Chorus");
    let perf = Rig::perf(&rig);
    assert_eq!(library_bpm(&rig, "No Other Name"), Some(91));
    assert_eq!((perf.song_index, perf.part_index), (1, 2), "the player did not move");
    assert_eq!(perf.songs[1].bpm, 91, "the set plays the new tempo");
    assert_eq!(perf.parts.len(), 5, "the new part is there");

    // An app save afterwards keeps the hand edit.
    Rig::edit_song(&rig, "What a God".into(), "What a God".into(), "A".into(), 80);
    let on_disk: SongLib = read(&songs_path);
    let nn = on_disk.songs.iter().find(|s| s.name == "No Other Name").unwrap();
    assert_eq!(nn.bpm, 91, "the rig's save did not revert the hand edit");
    assert_eq!(on_disk.songs.iter().find(|s| s.name == "What a God").unwrap().key, "A");

    // ── a save racing an edit the rig has not loaded yet: the file wins ──
    let mut lib: SongLib = read(&songs_path);
    lib.songs.iter_mut().find(|s| s.name == "Who Else").unwrap().bpm = 99;
    write(&songs_path, &lib);
    Rig::edit_song(&rig, "What a God".into(), "What a God".into(), "B".into(), 80);
    let on_disk: SongLib = read(&songs_path);
    assert_eq!(on_disk.songs.iter().find(|s| s.name == "Who Else").unwrap().bpm, 99);
    eventually("the racing edit reaches the rig", || library_bpm(&rig, "Who Else") == Some(99));
    // The auto-saves that follow (the pump flushes once a second) keep it.
    std::thread::sleep(Duration::from_millis(2500));
    let on_disk: SongLib = read(&songs_path);
    assert_eq!(on_disk.songs.iter().find(|s| s.name == "Who Else").unwrap().bpm, 99);
    assert_eq!(on_disk.songs.iter().find(|s| s.name == "No Other Name").unwrap().bpm, 91);

    // ── a file that does not parse: nothing reloads, nothing reseeds ──
    let broken = "songs ({name \"What a God\", key G, bpm\n";
    std::fs::write(&songs_path, broken).unwrap();
    let report = Rig::reload_config(&rig);
    assert!(report.contains("songs.styx: does not parse"), "{report}");
    assert_eq!(library_bpm(&rig, "No Other Name"), Some(91), "the running songs kept");
    Rig::edit_song(&rig, "What a God".into(), "What a God".into(), "C".into(), 80);
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(std::fs::read_to_string(&songs_path).unwrap(), broken, "never saved over, never reseeded");
    // Fixed: the next good save applies.
    let mut fixed = lib.clone();
    fixed.songs.iter_mut().find(|s| s.name == "Who Else").unwrap().bpm = 101;
    write(&songs_path, &fixed);
    eventually("the fixed file applies", || library_bpm(&rig, "Who Else") == Some(101));

    // ── setlists.styx reordered: the player follows its song by name ──
    let mut sets: SetlistLib = read(&sets_path);
    sets.setlists[0].entries.rotate_left(1);
    write(&sets_path, &sets);
    let report = Rig::reload_config(&rig);
    assert!(report.contains("setlists.styx: 2 setlists; on No Other Name / Chorus"), "{report}");
    let perf = Rig::perf(&rig);
    assert_eq!((perf.song_index, perf.part_index), (0, 2), "followed by name, not index");
    assert_eq!(perf.songs[0].name, "No Other Name");

    // ── the playing profile edited: its patches reload ──
    let worship = dir.join("profiles").join("worship.styx");
    let mut def: ProfileDef = read(&worship);
    let mut extra = def.patches[0].clone();
    extra.name = "Hot Reloaded".into();
    def.patches.push(extra);
    write(&worship, &def);
    logged(&log, "profiles/worship.styx: profile Worship:");
    assert!(Rig::patches(&rig).iter().any(|p| p.name == "Hot Reloaded"));
    // The rig's next save of the profile writes on top of the hand edit.
    Rig::rename_patch(&rig, "Clean Dry".into(), "Clean Dry 2".into());
    std::thread::sleep(Duration::from_millis(1500));
    let def: ProfileDef = read(&worship);
    assert!(def.patches.iter().any(|p| p.name == "Hot Reloaded"), "the hand edit kept");
    assert!(def.patches.iter().any(|p| p.name == "Clean Dry 2"), "the rig's edit saved");

    // ── `signal rig reload`'s request file is answered and removed ──
    let mut sets: SetlistLib = read(&sets_path);
    sets.setlists[0].name = "Renamed Set".into();
    write(&sets_path, &sets);
    std::fs::write(dir.join(signal_guitar::config_watch::RELOAD_REQUEST), "req-42").unwrap();
    eventually("the request is answered", || !dir.join(signal_guitar::config_watch::RELOAD_REQUEST).exists());
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("[req-42] setlists.styx: 2 setlists"), "{logged}");
    assert!(logged.contains("[req-42] reload requested: 1 file(s) applied"), "{logged}");
    assert_eq!(Rig::perf(&rig).setlists[0], "Renamed Set");

    let _ = std::fs::remove_dir_all(&root);
}
