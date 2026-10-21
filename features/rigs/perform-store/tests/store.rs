//! The store over a fake rig: a first start seeds a profile from the
//! presets; stacks rotate; songs, parts and sets play what they name; and
//! everything is there again after a restart.

use std::sync::Mutex;

use signal_rig_perform_store::{PerformStore, PresetPlayer};
use signal_rigs_proto::perform::Applied;

/// Two kits, two variations each; remembers what it was told to play.
#[derive(Default)]
struct Fake {
    played: Mutex<Vec<(String, String)>>,
}

impl PresetPlayer for Fake {
    fn presets(&self) -> Vec<(String, Vec<String>)> {
        vec![
            ("Rock".into(), vec!["Default".into(), "Room".into()]),
            ("Brush".into(), vec!["Default".into(), "Dry".into()]),
        ]
    }
    fn play(&self, preset: &str, variation: &str) -> Applied {
        if !self
            .presets()
            .iter()
            .any(|(p, v)| p == preset && v.iter().any(|x| x == variation))
        {
            return Applied::refused("no such preset");
        }
        self.played
            .lock()
            .unwrap()
            .push((preset.into(), variation.into()));
        Applied::done()
    }
}

impl Fake {
    fn last(&self) -> (String, String) {
        self.played
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("perform-store-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn a_first_start_seeds_a_profile_from_the_presets() {
    let rig = Fake::default();
    let store = PerformStore::open(dir("seed"), &rig);
    let s = store.state(&rig);
    assert_eq!(s.profile_name, "Default");
    assert_eq!(
        s.stacks.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
        ["Rock", "Brush"]
    );
    assert_eq!(s.stacks[0].patches, ["Rock · Default", "Rock · Room"]);
    assert_eq!(s.setlists, ["Set 1"]);
}

#[test]
fn a_stack_plays_then_rotates() {
    let rig = Fake::default();
    let store = PerformStore::open(dir("stack"), &rig);
    store.press_stack(&rig, 1);
    assert_eq!(rig.last(), ("Brush".into(), "Default".into()));
    store.press_stack(&rig, 1);
    assert_eq!(rig.last(), ("Brush".into(), "Dry".into()));
    let s = store.state(&rig);
    assert_eq!(s.active_patch, "Brush · Dry");
    assert!(s.stacks[1].is_active && !s.stacks[0].is_active);
}

#[test]
fn songs_and_parts_play_what_they_name_and_survive_a_restart() {
    let path = dir("songs");
    let rig = Fake::default();
    {
        let store = PerformStore::open(&path, &rig);
        store.add_song(&rig, "Holy", "G", 72);
        store.add_setlist_entry(&rig, 0, "Holy");
        assert!(store.select_song(&rig, 0).ok);
        store.add_part(&rig, "Verse");
        store.add_part(&rig, "Chorus");
        store.set_part_patch(&rig, "Verse", "Brush · Dry");
        store.set_part_patch(&rig, "Chorus", "Rock · Room");
        assert!(store.select_part(&rig, 1).ok);
        assert_eq!(rig.last(), ("Rock".into(), "Room".into()));
    }
    // A restart: the song, its parts, the set and where it was are kept.
    let store = PerformStore::open(&path, &rig);
    let s = store.state(&rig);
    assert_eq!(
        s.songs.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
        ["Holy"]
    );
    assert_eq!(s.songs[0].bpm, 72);
    assert_eq!(
        s.parts.iter().map(|p| p.patch.as_str()).collect::<Vec<_>>(),
        ["Brush · Dry", "Rock · Room"]
    );
    assert_eq!((s.part_index, s.active_patch.as_str()), (1, "Rock · Room"));
    // A song starts on its first part's patch.
    assert!(store.select_song(&rig, 0).ok);
    assert_eq!(rig.last(), ("Brush".into(), "Dry".into()));
}

#[test]
fn a_renamed_patch_is_followed_and_refusals_say_why() {
    let rig = Fake::default();
    let store = PerformStore::open(dir("rename"), &rig);
    store.add_song(&rig, "Holy", "G", 72);
    store.add_setlist_entry(&rig, 0, "Holy");
    assert!(store.select_song(&rig, 0).ok);
    store.add_part(&rig, "Verse");
    store.set_part_patch(&rig, "Verse", "Rock · Room");
    store.rename_patch(&rig, "Rock · Room", "Big Room");
    let s = store.state(&rig);
    assert_eq!(s.stacks[0].patches, ["Rock · Default", "Big Room"]);
    assert_eq!(s.parts[0].patch, "Big Room");
    assert!(!store.select_profile(&rig, "Nope").ok);
    assert!(!store.select_song(&rig, 9).ok);
    // The last profile and the last set stay.
    store.delete_profile(&rig, "Default");
    store.delete_setlist(&rig, 0);
    let s = store.state(&rig);
    assert_eq!((s.profile_name.as_str(), s.setlists.len()), ("Default", 1));
}

#[test]
fn a_profile_never_holds_a_patch_twice() {
    // A rig whose variations repeat a name (a library mix called Default).
    struct Twice;
    impl PresetPlayer for Twice {
        fn presets(&self) -> Vec<(String, Vec<String>)> {
            vec![("Kit".into(), vec!["Default".into(), "Room".into(), "Default".into()])]
        }
        fn play(&self, _: &str, _: &str) -> Applied {
            Applied::done()
        }
    }
    let path = dir("twice");
    let store = PerformStore::open(&path, &Twice);
    assert_eq!(store.state(&Twice).stacks[0].patches, ["Kit · Default", "Kit · Room"]);

    // A file that already holds one twice is repaired as it opens.
    let file = path.join("profiles.styx");
    let text = std::fs::read_to_string(&file).unwrap();
    let doubled = text.replacen("\"Kit · Room\"", "\"Kit · Room\" \"Kit · Room\"", 1);
    assert_ne!(text, doubled, "the stack line is where the test expects");
    std::fs::write(&file, doubled).unwrap();
    let store = PerformStore::open(&path, &Twice);
    assert_eq!(store.state(&Twice).stacks[0].patches, ["Kit · Default", "Kit · Room"]);
}
