//! Seek semantics over the standalone backend (no REAPER).
//!
//! Guards the browser-setlist seek regressions:
//!
//! * a song-relative `seek_to` must LAND — including while paused (the bug:
//!   a bare `set_position` with no `refresh_active_indices` /
//!   `publish_transport_snapshot` meant a paused seek never republished, and
//!   Play "resumed" at the stale position), and
//! * a `seek_to` may NEVER leave the current song — an out-of-range target
//!   (bad measure math, stale section list, replayed RPC) must clamp inside
//!   the song instead of running the playhead into the next song on the
//!   shared timeline ("clicking the section bar jumped me to another song").
//!
//! Run: cargo test -p session --test `seek_semantics_tests` -- --nocapture

use daw_proto::transport::service::Transport;
use daw_proto::{ProjectContext, ProjectInfo};
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;
use session::SetlistServiceImpl;
use session::services::SetlistService;
use session::setlist::service::demo::stamp_demo_setlist_with;

fn seeded_stamped() -> eyre::Result<Standalone> {
    let standalone = Standalone::new();
    standalone.seed_project(ProjectInfo {
        guid: "demo-proj".into(),
        name: "Demo".into(),
        path: String::new(),
    });
    stamp_demo_setlist_with(&standalone)?;
    Ok(standalone)
}

/// One test fn (not several) because `daw::init_from_parts` installs a
/// process-global facade.
#[test]
fn seek_to_lands_and_never_leaves_the_song() -> eyre::Result<()> {
    // Manual runtime with roomy stacks — same rationale as the over-vox
    // harness (vox debug-build encode recursion on Setlist payloads).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    rt.block_on(inner())
}

async fn inner() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let standalone = seeded_stamped()?;
    let bundle = build_in_process_daw(standalone.clone()).await?;
    let runtime = std::sync::Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), runtime);

    let svc = SetlistServiceImpl::with_daw(standalone.clone());
    svc.build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;

    let setlist = svc.setlist().map_err(|e| eyre::eyre!("setlist: {e:?}"))?;
    assert!(
        setlist.songs.len() >= 2,
        "need ≥2 songs to prove seeks can't cross songs (got {})",
        setlist.songs.len()
    );
    let song0 = setlist
        .songs
        .first()
        .ok_or_else(|| eyre::eyre!("no songs in setlist"))?
        .clone();
    let dur0 = song0.duration();
    assert!(dur0 > 1.0, "demo song 0 too short to test ({dur0:.2}s)");
    let ctx = ProjectContext::Project(song0.project_guid.clone());
    let pos = |sa: &Standalone| Transport::get_position(sa, ctx.clone());

    // Open on song 0 / section 0 — the browser bridge's initial cursor seed.
    svc.seek_to_section(0, 0)
        .await
        .map_err(|e| eyre::eyre!("seek_to_section: {e:?}"))?;
    let active = svc
        .active_song()
        .map_err(|e| eyre::eyre!("active_song: {e:?}"))?;
    assert_eq!(active.name, song0.name, "opened on song 0");

    // ── 1. A PAUSED mid-song seek must land ────────────────────────────────
    let mid = dur0 * 0.5;
    svc.seek_to(mid)
        .await
        .map_err(|e| eyre::eyre!("seek_to(mid): {e:?}"))?;
    let p = pos(&standalone);
    let want = song0.start_seconds() + mid;
    assert!(
        (p - want).abs() < 0.5,
        "paused seek_to({mid:.2}) must land at {want:.2}s abs (got {p:.2}s)"
    );
    let active = svc
        .active_song()
        .map_err(|e| eyre::eyre!("active_song: {e:?}"))?;
    assert_eq!(active.name, song0.name, "mid-song seek stays on song 0");

    // ── 2. An out-of-range seek CLAMPS inside the song (never crosses) ─────
    for wild in [dur0 + 5.0, dur0 * 40.0, 1.0e12] {
        svc.seek_to(wild)
            .await
            .map_err(|e| eyre::eyre!("seek_to({wild}): {e:?}"))?;
        let p = pos(&standalone);
        assert!(
            p < song0.end_seconds(),
            "seek_to({wild:.1}) must clamp INSIDE song 0 (end {:.2}s), got {p:.2}s",
            song0.end_seconds()
        );
        let active = svc
            .active_song()
            .map_err(|e| eyre::eyre!("active_song: {e:?}"))?;
        assert_eq!(
            active.name, song0.name,
            "seek_to({wild:.1}) must not jump to another song"
        );
    }

    // ── 3. A negative seek clamps to the song start ────────────────────────
    svc.seek_to(-42.0)
        .await
        .map_err(|e| eyre::eyre!("seek_to(-42): {e:?}"))?;
    let p = pos(&standalone);
    assert!(
        (p - song0.start_seconds()).abs() < 0.5,
        "negative seek clamps to the song start ({:.2}s), got {p:.2}s",
        song0.start_seconds()
    );

    println!(
        "[seek-semantics] song 0 '{}' ({dur0:.1}s): paused seek landed, wild + negative \
         seeks clamped in-song",
        song0.name
    );
    Ok(())
}
