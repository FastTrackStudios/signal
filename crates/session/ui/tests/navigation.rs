//! Integration test: clicking around the setlist navigator must select the
//! song/section that was actually clicked.
//!
//! Drives the REAL `PerformanceSidebar` component (the one both
//! `apps/desktop` and `apps/web` ship) against a real in-process
//! `daw-standalone` + `session` setlist backend — same bootstrap shape as
//! `apps/web/src/demo_backend.rs` and
//! `session::setlist_service_over_vox`/`standalone_setlist_harness` — through
//! `dioxus-test`'s headless blitz-dom renderer (no browser, no desktop app
//! window). A bug here is a bug in the shared component, not in one host's
//! wiring around it.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use daw::service::ProjectContext;
use daw::LayerRouter;
use daw_proto::ProjectInfo;
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;
use dioxus::prelude::*;
use dioxus_test::{by_testid, render, DocumentTester};
use session::services::setlist_service::{
    setlist_service_stream_service_descriptor, stream_serve as setlist_service_stream_serve,
    SetlistServiceStreamClient,
};
use session::setlist::service::demo::{fixture_songs, stamp_song_native};
use session::{
    serve_setlist_service, setlist_service_service_descriptor, SetlistServiceClient,
    SetlistServiceImpl,
};
use session_ui::{PerformanceSidebar, ACTIVE_INDICES, SETLIST_STRUCTURE};

/// The stream client the test root's event-pump `use_future` reads from.
/// `render()` takes a bare `fn() -> Element`, so this is how the
/// already-booted backend reaches the component tree — the same role
/// `Session::init` plays for the RPC client.
static STREAM_CLIENT: OnceLock<SetlistServiceStreamClient> = OnceLock::new();

/// Boots a real in-process backend seeded with 3 fixture songs (5+ sections
/// each — `fixture_songs`'s layouts cycle through increasing complexity) and
/// installs it as `session_ui::Session`. Mirrors
/// `apps/web/src/demo_backend.rs::boot` minus the wasm/native cfg-split
/// (this test only ever runs natively).
async fn boot() {
    let standalone = Standalone::new();
    for (i, song) in fixture_songs(3).iter().enumerate() {
        let guid = standalone.seed_project(ProjectInfo {
            guid: format!("nav-test-song-{i:02}"),
            name: song.name.to_string(),
            path: String::new(),
        });
        stamp_song_native(&standalone, &ProjectContext::Project(guid), song)
            .unwrap_or_else(|e| panic!("stamp song {i} ({}): {e:?}", song.name));
    }

    let bundle = build_in_process_daw(standalone.clone())
        .await
        .expect("build_in_process_daw");
    let block_on_rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("block_on runtime"),
    );
    daw::init_from_parts(bundle.daw.clone(), block_on_rt);

    let setlist = SetlistServiceImpl::with_daw(standalone);
    let router = LayerRouter::new()
        .with(
            setlist_service_service_descriptor(),
            serve_setlist_service(setlist.clone()),
        )
        .with(
            setlist_service_stream_service_descriptor(),
            setlist_service_stream_serve(setlist.clone()),
        );
    let scope = Arc::new(architect::Scope::new());
    let server = architect::LocalServer::serve(router, Arc::clone(&scope));
    let caller = server.caller().await.expect("local setlist caller");
    let client = SetlistServiceClient::new(caller);
    let stream_client = server
        .establish::<SetlistServiceStreamClient>()
        .await
        .expect("local setlist stream client");

    setlist.start_stream_pumps();
    client
        .build_from_open_projects()
        .await
        .expect("build_from_open_projects");

    session_ui::Session::init(client).expect("Session::init");
    STREAM_CLIENT.set(stream_client).ok();

    // Keep the in-process link alive for the test's lifetime — this test
    // process only ever boots one backend, so leaking is the simplest
    // correct answer (matches demo_backend.rs's Arc-held-forever shape,
    // minus the cache since there's only ever one boot here).
    std::mem::forget(bundle);
    std::mem::forget(scope);
}

/// The pumps `apps/web/src/routes/demo.rs`'s `Demo` component runs, trimmed
/// to what `PerformanceSidebar` actually reads: `SETLIST_STRUCTURE` (from
/// the initial snapshot + `SetlistChanged` events) and `ACTIVE_INDICES`
/// (from the `active_indices` stream).
#[component]
fn TestRoot() -> Element {
    use_future(move || async move {
        let stream_client = STREAM_CLIENT.get().expect("boot() ran before render()");

        let (tx, mut rx) = vox::channel::<session::SetlistEvent>();
        let events_stream_client = stream_client.clone();
        spawn(async move {
            let _ = events_stream_client.events(tx).await;
        });
        if let Ok(setlist) = session_ui::Session::get().setlist().setlist().await {
            session_ui::apply_setlist_event(&session::SetlistEvent::SetlistChanged(setlist));
        }
        spawn(async move {
            while let Ok(Some(ev)) = rx.recv().await {
                session_ui::apply_setlist_event(ev.get());
            }
        });

        let (tx, mut rx) = vox::channel::<session::ActiveIndices>();
        let indices_stream_client = stream_client.clone();
        spawn(async move {
            let _ = indices_stream_client.active_indices(tx).await;
        });
        // Open on song 0 / section 0, same as `apps/web`'s `Demo` component
        // — the standalone backend's edit cursor does not default to the
        // first song (in this fixture it lands on the LAST-seeded one), so
        // without this the test would start from an arbitrary song.
        spawn(async move {
            let _ = session_ui::Session::get()
                .setlist()
                .seek_to_section(0, 0)
                .await;
        });
        spawn(async move {
            while let Ok(Some(ai)) = rx.recv().await {
                session_ui::apply_active_indices(ai.get());
            }
        });
    });

    // `SETLIST_STRUCTURE`/`ACTIVE_INDICES` are Dioxus `GlobalSignal`s —
    // reading them requires an active Dioxus runtime, which the test
    // function's own async body does not have (only component bodies and
    // dioxus-spawned tasks do). Surface the state the test needs to assert
    // on as plain DOM attributes instead, read back through the tester's
    // normal query path (test_that::matchers::eq/some), and read INSIDE
    // this component where the runtime is present.
    let active = ACTIVE_INDICES.read();
    let song_index_attr = active.song_index.map(|i| i.to_string()).unwrap_or_default();
    let section_index_attr = active
        .section_index
        .map(|i| i.to_string())
        .unwrap_or_default();
    drop(active);
    let song_count_attr = SETLIST_STRUCTURE.read().songs.len().to_string();

    rsx! {
        div {
            "data-testid": "debug-active",
            "data-song-index": "{song_index_attr}",
            "data-section-index": "{section_index_attr}",
            "data-song-count": "{song_count_attr}",
        }
        PerformanceSidebar {}
    }
}

/// Reads one `data-*` attribute off the `debug-active` element (see
/// `TestRoot`) — the only way to observe `ACTIVE_INDICES`/
/// `SETLIST_STRUCTURE` from outside the Dioxus runtime the test function's
/// own async body doesn't have.
async fn debug_attr(tester: &DocumentTester, name: &str) -> Option<String> {
    tester
        .query(by_testid("debug-active"))
        .await
        .expect("debug-active element")
        .attribute(name)
        .filter(|s| !s.is_empty())
}

/// Pumps the tester's event loop until `pred` (evaluated against the
/// `debug-active` element's current attributes) is true, or panics.
async fn wait_until(tester: &DocumentTester, mut pred: impl AsyncFnMut(&DocumentTester) -> bool) {
    for _ in 0..300 {
        if pred(tester).await {
            return;
        }
        let _ = tester.pump().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "condition never became true (song={:?} section={:?} count={:?})",
        debug_attr(tester, "data-song-index").await,
        debug_attr(tester, "data-section-index").await,
        debug_attr(tester, "data-song-count").await,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clicking_a_section_selects_the_song_it_belongs_to() {
    let _ = tracing_subscriber::fmt::try_init();
    boot().await;

    let tester = render(TestRoot).build();

    // Wait for the setlist snapshot to arrive (3 songs).
    wait_until(&tester, async |t| {
        debug_attr(t, "data-song-count").await.as_deref() == Some("3")
    })
    .await;
    // Establish a known baseline (song 0 / section 0) — the backend's
    // default active song is otherwise whichever project last became
    // "current" while seeding, not song 0.
    wait_until(&tester, async |t| {
        debug_attr(t, "data-song-index").await.as_deref() == Some("0")
            && debug_attr(t, "data-section-index").await.as_deref() == Some("0")
    })
    .await;

    // Click song 0's row: expands it (and makes its sections clickable —
    // SongItem only renders a song's section list while it is expanded).
    tester
        .query("[data-testid=\"sidebar-song-click-0\"] div")
        .click()
        .await
        .expect("song 0 row");
    let _ = tester.pump().await;
    wait_until(&tester, async |t| {
        debug_attr(t, "data-song-index").await.as_deref() == Some("0")
    })
    .await;

    // Click section 2 of song 0 (now visible) and confirm we land on
    // (song 0, section 2) — not moved to a different song.
    tester
        .query("[data-testid=\"sidebar-section-0-2\"] div")
        .click()
        .await
        .expect("song 0 section 2");
    let _ = tester.pump().await;
    wait_until(&tester, async |t| {
        debug_attr(t, "data-section-index").await.as_deref() == Some("2")
    })
    .await;
    assert_eq!(
        debug_attr(&tester, "data-song-index").await.as_deref(),
        Some("0"),
        "clicking a section inside song 0 must not move the active song"
    );

    // Now switch to song 1 and click ONE of its sections — the actual
    // regression: this must select song 1, not stay on (or fall back to)
    // song 0.
    tester
        .query("[data-testid=\"sidebar-song-click-1\"] div")
        .click()
        .await
        .expect("song 1 row");
    let _ = tester.pump().await;
    wait_until(&tester, async |t| {
        debug_attr(t, "data-song-index").await.as_deref() == Some("1")
    })
    .await;

    tester
        .query("[data-testid=\"sidebar-section-1-1\"] div")
        .click()
        .await
        .expect("song 1 section 1");
    let _ = tester.pump().await;
    wait_until(&tester, async |t| {
        debug_attr(t, "data-section-index").await.as_deref() == Some("1")
            && debug_attr(t, "data-song-index").await.as_deref() == Some("1")
    })
    .await;

    let song_index = debug_attr(&tester, "data-song-index").await;
    assert_eq!(
        song_index.as_deref(),
        Some("1"),
        "clicking a section in song 1 must select song 1, not song {song_index:?}"
    );
    assert_eq!(
        debug_attr(&tester, "data-section-index").await.as_deref(),
        Some("1")
    );
}
