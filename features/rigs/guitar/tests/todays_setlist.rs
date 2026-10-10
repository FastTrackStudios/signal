//! A setlist dated today opens on its own when the rig does: Setlist mode,
//! that set, its first song.

use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::Rig;

#[test]
fn the_setlist_dated_today_opens_with_the_rig() {
    let root = std::env::temp_dir().join(format!("todays-setlist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("rig");
    std::fs::create_dir_all(&dir).unwrap();
    let today = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
    std::fs::write(
        dir.join("setlists.styx"),
        format!(
            "setlists ({{name \"Tonight\", date \"{today}\", entries ({{song WASHED, key \"\", bpm 0}} {{song TAKEOVER, key \"\", bpm 0}})}})\n"
        ),
    )
    .unwrap();
    // SAFETY: the only test in this binary; nothing else reads the env yet.
    unsafe {
        std::env::set_var("SIGNAL_RIG_DIR", &dir);
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
    }
    let rig = GuitarRigBackend::new();
    rig.open_blocking();
    let perf = Rig::perf(&rig);
    assert_eq!(perf.perform_mode, 2, "Setlist mode");
    assert_eq!(perf.setlists[perf.setlist_index as usize], "Tonight");
    assert_eq!(perf.song_index, 0);
}
