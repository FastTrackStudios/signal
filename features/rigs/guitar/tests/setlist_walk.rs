//! Every shipped setlist, walked song by song on a design-mode rig over a
//! throwaway library: each song selects, plays a patch, and leaves the rig
//! with a tempo — a song with none of its own keeps the one playing (a tempo
//! of 0 worked the delays' times and synced reverbs' decays out from
//! 60 000 ms ÷ 0).

use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::Rig;

#[test]
fn every_shipped_setlist_walks_through_with_a_tempo() {
    let root = std::env::temp_dir().join(format!("setlist-walk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the only test in this binary; nothing else reads the env yet.
    unsafe {
        std::env::set_var("SIGNAL_RIG_DIR", root.join("rig"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
    }
    let rig = GuitarRigBackend::new();
    rig.open_blocking();
    Rig::set_perform_mode(&rig, 2);
    let sets = Rig::perf(&rig).setlists;
    assert!(sets.iter().any(|s| s.starts_with("HSM 10-6-26")), "{sets:?}");
    for (i, set) in sets.iter().enumerate() {
        let a = Rig::select_setlist(&rig, i as u32);
        assert!(a.ok, "{set}: {}", a.message);
        let songs = Rig::perf(&rig).songs;
        assert!(!songs.is_empty(), "{set} has songs");
        for (k, song) in songs.iter().enumerate() {
            let a = Rig::select_song(&rig, k as u32);
            assert!(a.ok, "{set} / {}: {}", song.name, a.message);
            assert!(
                Rig::patches(&rig).iter().any(|p| p.active),
                "{set} / {}: a patch plays",
                song.name
            );
            assert!(
                Rig::perf(&rig).tempo_bpm > 0,
                "{set} / {}: left the rig at tempo 0",
                song.name
            );
        }
    }
}
