//! The player following a library served over a real WebSocket: edits on
//! the server's disk swap in place without losing the running state, and
//! a malformed edit is refused with a toast.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use architect::Services as _;
use library_fs::{FsLibrary, SurfaceStore as _};
use library_link::Served;
use surface_memory::Memory;
use surface_runtime::Modifiers;

use super::*;
use crate::player::Backend;

/// Real `.frame` files carry node ids, so an edit keeps them (and the
/// running state attached to them).
const PANEL: &str = r##"<Document id="00000000-0000-4000-8000-000000000001"><Page id="00000000-0000-4000-8000-000000000002"><Frame name="Live" id="00000000-0000-4000-8000-000000000003" w=300 h=160 fill="#101010"><Frame name="Gain" id="00000000-0000-4000-8000-000000000004" x=20 y=20 w=80 h=80 control={{"binding":{"curve":"Linear","max":1.0,"min":0.0,"source":{"Param":{"addr":"amp/gain"}}},"kind":{"Knob":{"sweep_degrees":270.0}},"slots":[]}} /></Frame></Page></Document>"##;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("frame-libview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tick_until(
    view: &mut LibraryView,
    player: &mut Player,
    want: impl Fn(&Outcome) -> bool,
) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(o) = view.tick(player).into_iter().find(|o| want(o)) {
            return o;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no matching outcome within 5 s");
}

fn gain(player: &Player) -> f64 {
    player
        .engine_value(&surface_proto::ParamAddr::new("amp/gain"))
        .unwrap()
}

#[test]
fn edits_swap_in_place_and_bad_ones_are_refused() {
    let dir = Dir::new("live");
    let file = dir.0.join("live.frame");
    std::fs::write(&file, PANEL).unwrap();
    let lib = FsLibrary::open(&dir.0).unwrap();
    let _watch = lib.watch_every(Duration::from_millis(20));
    let served = Served::start(
        lib.clone().into_router(),
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
    )
    .unwrap();

    let installs = dir.0.join(".installed");
    let mut view =
        LibraryView::start(&served.url, Some("live".into()), false, installs.clone()).unwrap();
    assert_eq!(view.mode(), Mode::Preview);
    let surface = view
        .initial()
        .unwrap()
        .expect("preview mode starts on the surface");
    let mut player = Player::new(Backend::Local(Memory::demo()), surface).unwrap();

    // Grab the knob and drag it partway: a gesture in flight.
    let (_, b) = player.first_knob().unwrap();
    let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let m = Modifiers::default();
    let before = gain(&player);
    player.pointer_down((cx, cy), (cx, cy), m, 0.0);
    player.pointer_move((cx, cy - 40.0), m, 16.0);
    player.tick(200.0);
    let mid = gain(&player);
    assert!(mid > before, "{before} → {mid}");

    // The design changes on the server's disk (an agent's save): it swaps in
    // under the hand — same values, the drag goes on.
    std::fs::write(
        &file,
        PANEL
            .replace("\"Live\"", "\"Edited\"")
            .replace("fill=\"#101010\"", "fill=\"#202020\""),
    )
    .unwrap();
    let o = tick_until(&mut view, &mut player, |o| {
        matches!(o, Outcome::Applied { .. })
    });
    assert_eq!(
        o,
        Outcome::Applied {
            id: "live".into(),
            version: 2
        }
    );
    assert_eq!(
        library_link::scene_name(player.scene()).as_deref(),
        Some("Edited")
    );
    assert!(
        (gain(&player) - mid).abs() < 1e-9,
        "param value kept across the swap"
    );
    player.pointer_move((cx, cy - 60.0), m, 300.0);
    player.pointer_up(316.0);
    player.tick(600.0);
    assert!(
        gain(&player) > mid,
        "the same drag continued after the swap"
    );
    let kept = gain(&player);

    // A malformed save is refused: the stage keeps the last good version.
    std::fs::write(&file, "<Document>\n  <Page>\n    <Frame name=\"Half").unwrap();
    let o = tick_until(&mut view, &mut player, |o| {
        matches!(o, Outcome::Rejected(_))
    });
    let Outcome::Rejected(why) = o else {
        unreachable!()
    };
    assert!(why.starts_with("live v3 rejected: "), "{why}");
    assert_eq!(
        library_link::scene_name(player.scene()).as_deref(),
        Some("Edited"),
        "not blanked"
    );
    assert!(view.toast_text().is_some_and(|t| t.error && t.text == why));
    assert!(view.toast(1000.0, 800.0).is_some(), "the toast draws");
    assert!((gain(&player) - kept).abs() < 1e-9);

    // The fix goes live again.
    std::fs::write(&file, PANEL.replace("\"Live\"", "\"Fixed\"")).unwrap();
    tick_until(&mut view, &mut player, |o| {
        matches!(o, Outcome::Applied { version: 4, .. })
    });
    assert_eq!(
        library_link::scene_name(player.scene()).as_deref(),
        Some("Fixed")
    );

    // Install copies it out; fork publishes a copy and previews that.
    view.install();
    assert_eq!(
        std::fs::read_to_string(installs.join("live.frame")).unwrap(),
        PANEL.replace("\"Live\"", "\"Fixed\"")
    );
    view.fork(&mut player);
    assert_eq!(
        view.title(),
        format!(
            "frame — fixed-fork — live from {} — B for the library",
            served.url
        )
    );
    let fork = lib.get("fixed-fork".into(), None).unwrap();
    assert_eq!(fork.meta.forked_from.as_deref(), Some("live@4"));

    // Back to the grid: both are listed.
    view.back();
    assert!(view.browsing());
    tick_until(&mut view, &mut player, |o| matches!(o, Outcome::Changed));
    let (scene, _) = view.document().unwrap();
    assert!(
        scene.nodes.values().any(|n| n.name == "fixed-fork"),
        "the fork's tile"
    );
}

#[test]
fn browse_mode_opens_a_surface_by_click() {
    let dir = Dir::new("browse");
    std::fs::create_dir_all(dir.0.join("a")).unwrap();
    std::fs::write(
        dir.0.join("a/one.frame"),
        PANEL.replace("\"Live\"", "\"One\""),
    )
    .unwrap();
    std::fs::write(
        dir.0.join("a/two.frame"),
        PANEL.replace("\"Live\"", "\"Two\""),
    )
    .unwrap();
    let lib = FsLibrary::open(&dir.0).unwrap();
    let served = Served::start(
        lib.into_router(),
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
    )
    .unwrap();
    let mut view = LibraryView::start(&served.url, None, true, dir.0.join(".i")).unwrap();
    assert!(view.browsing());
    assert!(view.initial().unwrap().is_none());
    let mut player =
        Player::new(Backend::Local(Memory::demo()), crate::source::placeholder()).unwrap();
    view.nav(crate::browser::Nav::Right);
    view.open_selected(&mut player);
    assert_eq!(view.mode(), Mode::Preview);
    assert_eq!(
        library_link::scene_name(player.scene()).as_deref(),
        Some("Two")
    );
    view.back();
    // The first tile sits at the grid's first slot.
    assert!(view.click((40.0, 130.0), &mut player));
    assert_eq!(
        library_link::scene_name(player.scene()).as_deref(),
        Some("One")
    );
}
