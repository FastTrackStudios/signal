//! signal's rig on a guitar track, by action: loaded with the guitar's
//! Worship profile it names the track for the stack it plays; a tone set
//! plays now; a tone switch put in at bar 2 is a step on its Patch lane.
//! A song of one track opened silent, its rig's library a folder of the
//! test's own.

#![cfg(feature = "signal")]

use session_daw::actions::list;

/// The action's answer, as JSON.
fn run(id: &str, args: &serde_json::Value) -> serde_json::Value {
    let out = list()
        .run_json(id, args.to_string().as_bytes())
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_slice(&out).unwrap_or_else(|e| panic!("{id} said: {e}"))
}

#[test]
fn a_rig_loaded_names_its_track_and_its_tones_are_set_and_switched() {
    let root = std::env::temp_dir().join(format!("rig-actions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a folder");
    // SAFETY: the only test in this binary, before any rig starts.
    unsafe {
        std::env::set_var("FTS_SIGNAL_RIG_ROOT", root.join("rigs"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
        std::env::set_var("SIGNAL_RELOAD_LOG", root.join("config-reload.log"));
        std::env::set_var("SIGNAL_DROP_LOG", root.join("dropouts.log"));
        std::env::remove_var("SIGNAL_RIG_DESIGN");
    }
    let song = root.join("song.RPP");
    std::fs::write(
        &song,
        "<REAPER_PROJECT 0.1 \"7.0\" 0\n  TEMPO 120 4 4\n  <TRACK\n    NAME Guitar\n  >\n>\n",
    )
    .expect("the song");
    session_daw::open::open_silent(&song).expect("the song opens");

    let loaded = run(
        "RIG_LOAD",
        &serde_json::json!({ "track": "Guitar", "profile": "Worship" }),
    );
    assert_eq!(loaded["profile"], "Worship", "{loaded}");
    let stack = loaded["stack"].as_str().expect("a stack plays").to_owned();
    let name = format!("Guitar {stack}");
    assert_eq!(
        loaded["track"],
        name.as_str(),
        "named for its stack: {loaded}"
    );
    let patches: Vec<String> = loaded["patches"]
        .as_array()
        .expect("its patches")
        .iter()
        .filter_map(|p| p.as_str().map(str::to_owned))
        .collect();
    assert!(patches.len() > 1, "{patches:?}");

    // A click on another tone: it plays now.
    let other = patches
        .iter()
        .find(|p| loaded["patch"].as_str() != Some(p.as_str()))
        .expect("another patch")
        .clone();
    let set = run(
        "TONE_SET",
        &serde_json::json!({ "track": name, "patch": other }),
    );
    assert_eq!(set["patch"], other.as_str(), "{set}");

    // A Shift-click: a switch put into the song at bar 2.
    let first = patches.first().expect("a patch").clone();
    run(
        "TONE_INSERT",
        &serde_json::json!({ "track": name, "patch": first, "at": "2" }),
    );
    let lanes = run("AUTOMATION_LANES", &serde_json::json!({ "track": name }));
    let text = lanes.to_string();
    assert!(text.contains("Patch"), "the rig's Patch lane: {text}");
    assert!(text.contains(&first), "its switch to {first}: {text}");

    // A wrong tone is refused, and says why.
    let refused = list().run_json(
        "TONE_SET",
        serde_json::json!({ "track": name, "patch": "No Such Tone" })
            .to_string()
            .as_bytes(),
    );
    assert!(refused.is_err(), "no such patch");
}
