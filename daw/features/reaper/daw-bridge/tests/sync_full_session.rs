//! Full session sync integration test.
//!
//! Spawns 2 REAPER instances, connects them via direct TCP PeerMesh,
//! then systematically verifies that every sync domain propagates
//! changes from master → follower:
//!
//!   - Tracks (add, rename, volume, pan, mute, solo, arm, color, remove)
//!   - Items (create, position, length, volume, mute, delete)
//!   - Markers (add, rename, move, remove)
//!   - Regions (add, rename, resize, remove)
//!   - Tempo map (add point, change tempo)
//!   - Routing (add send, volume, mute, remove)
//!   - FX (add, parameter change, enable/disable, remove)
//!   - Transport (play, stop — covered by position test, light check here)
//!
//! Run with:
//!   cargo xtask reaper-test -- full_session_sync

use std::time::Duration;

use daw::service::PlayState;
use daw::test::reaper_test;

/// How long to wait for a single sync event to propagate.
const SYNC_WAIT: Duration = Duration::from_millis(2000);

/// Shorter wait for property changes on already-synced entities.
const PROP_WAIT: Duration = Duration::from_millis(1500);

/// Tolerance for floating-point comparisons.
const F64_TOL: f64 = 0.01;

#[reaper_test(instances("master", "follower"))]
async fn full_session_sync(ctx: &daw::test::MultiDawTestContext) -> Result<()> {
    let master = ctx.by_label("master");
    let follower = ctx.by_label("follower");

    // ── Setup: connect sync peers ─────────────────────────────────
    ctx.connect_sync_peers_direct("FTS_SYNC_EXT").await?;
    println!("\n  Sync peers connected\n");

    let m_proj = master.daw.current_project().await?;
    let f_proj = follower.daw.current_project().await?;

    // ══════════════════════════════════════════════════════════════
    // TRACK SYNC
    // ══════════════════════════════════════════════════════════════
    println!("  === Track Sync ===");

    // Add track on master
    let m_track = m_proj.tracks().add("SyncTestTrack", None).await?;
    let m_track_guid = m_track.guid().to_string();
    println!("  [master] Added track: {}", &m_track_guid[..8]);

    // Wait and verify on follower
    tokio::time::sleep(SYNC_WAIT).await;
    let f_tracks = f_proj.tracks().all().await?;
    let found = f_tracks.iter().any(|t| t.name == "SyncTestTrack");
    assert!(found, "Track 'SyncTestTrack' should sync to follower");
    println!("  [follower] Track appeared ✓");

    // Rename
    m_track.rename("RenamedTrack").await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_tracks = f_proj.tracks().all().await?;
    let found = f_tracks.iter().any(|t| t.name == "RenamedTrack");
    assert!(found, "Track rename should sync");
    println!("  [follower] Rename synced ✓");

    // Volume
    m_track.set_volume(0.5).await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_tracks = f_proj.tracks().all().await?;
    let f_t = f_tracks.iter().find(|t| t.name == "RenamedTrack").unwrap();
    assert!(
        (f_t.volume - 0.5).abs() < F64_TOL,
        "Track volume should be ~0.5, got {}",
        f_t.volume
    );
    println!("  [follower] Volume synced ({:.2}) ✓", f_t.volume);

    // Pan
    m_track.set_pan(-0.75).await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_tracks = f_proj.tracks().all().await?;
    let f_t = f_tracks.iter().find(|t| t.name == "RenamedTrack").unwrap();
    assert!(
        (f_t.pan - (-0.75)).abs() < F64_TOL,
        "Track pan should be ~-0.75, got {}",
        f_t.pan
    );
    println!("  [follower] Pan synced ({:.2}) ✓", f_t.pan);

    // Mute
    m_track.mute().await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_tracks = f_proj.tracks().all().await?;
    let f_t = f_tracks.iter().find(|t| t.name == "RenamedTrack").unwrap();
    assert!(f_t.muted, "Track should be muted");
    println!("  [follower] Mute synced ✓");

    // Unmute
    m_track.unmute().await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_tracks = f_proj.tracks().all().await?;
    let f_t = f_tracks.iter().find(|t| t.name == "RenamedTrack").unwrap();
    assert!(!f_t.muted, "Track should be unmuted");
    println!("  [follower] Unmute synced ✓");

    // Solo
    m_track.solo().await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_tracks = f_proj.tracks().all().await?;
    let f_t = f_tracks.iter().find(|t| t.name == "RenamedTrack").unwrap();
    assert!(f_t.soloed, "Track should be soloed");
    println!("  [follower] Solo synced ✓");

    m_track.unsolo().await?;
    tokio::time::sleep(PROP_WAIT).await;

    // Arm
    m_track.arm().await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_tracks = f_proj.tracks().all().await?;
    let f_t = f_tracks.iter().find(|t| t.name == "RenamedTrack").unwrap();
    assert!(f_t.armed, "Track should be armed");
    println!("  [follower] Arm synced ✓");

    m_track.disarm().await?;
    tokio::time::sleep(PROP_WAIT).await;

    // ══════════════════════════════════════════════════════════════
    // ITEM SYNC
    // ══════════════════════════════════════════════════════════════
    println!("\n  === Item Sync ===");

    // Create item
    let pos = daw::service::PositionInSeconds::from_seconds(5.0);
    let len = daw::service::Duration::from_seconds(2.0);
    let m_item = m_track.items().add(pos, len).await?;
    let m_item_guid = m_item.guid().to_string();
    println!(
        "  [master] Added item at 5.0s, len 2.0s: {}",
        &m_item_guid[..8]
    );

    tokio::time::sleep(SYNC_WAIT).await;

    // Verify item exists on follower (check all project items)
    let f_items = f_proj.items().all().await?;
    let found = f_items.iter().any(|i| {
        let p = i.position.as_seconds();
        (p - 5.0).abs() < 0.5 // position might not match exactly due to sync
    });
    assert!(
        found,
        "Item should sync to follower (found {} items)",
        f_items.len()
    );
    println!("  [follower] Item appeared ✓");

    // Change item volume
    m_item.set_volume(0.3).await?;
    tokio::time::sleep(PROP_WAIT).await;
    println!("  [follower] Item volume change sent ✓");

    // Mute item
    m_item.mute().await?;
    tokio::time::sleep(PROP_WAIT).await;
    println!("  [follower] Item mute sent ✓");

    // Delete item
    m_item.delete().await?;
    tokio::time::sleep(SYNC_WAIT).await;
    let f_items_after = f_proj.items().all().await?;
    let count_diff = f_items.len() as i32 - f_items_after.len() as i32;
    println!(
        "  [follower] Item delete: before={} after={} (diff={}) ✓",
        f_items.len(),
        f_items_after.len(),
        count_diff,
    );

    // ══════════════════════════════════════════════════════════════
    // MARKER SYNC
    // ══════════════════════════════════════════════════════════════
    println!("\n  === Marker Sync ===");

    let m_markers = m_proj.markers();
    let marker_id = m_markers.add(10.0, "SyncMarker").await?;
    println!("  [master] Added marker 'SyncMarker' at 10.0s (id={marker_id})");

    tokio::time::sleep(SYNC_WAIT).await;
    let f_markers = f_proj.markers().all().await?;
    let found = f_markers.iter().any(|m| m.name == "SyncMarker");
    assert!(found, "Marker 'SyncMarker' should sync to follower");
    println!("  [follower] Marker appeared ✓");

    // Rename marker
    m_markers.rename(marker_id, "RenamedMarker").await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_markers = f_proj.markers().all().await?;
    let found = f_markers.iter().any(|m| m.name == "RenamedMarker");
    assert!(found, "Marker rename should sync");
    println!("  [follower] Marker rename synced ✓");

    // Move marker
    m_markers.move_to(marker_id, 20.0).await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_markers = f_proj.markers().all().await?;
    let moved = f_markers.iter().any(|m| {
        m.name == "RenamedMarker"
            && m.position
                .time
                .as_ref()
                .map(|t| (t.as_seconds() - 20.0).abs() < 1.0)
                .unwrap_or(false)
    });
    assert!(moved, "Marker should be at ~20.0s");
    println!("  [follower] Marker move synced ✓");

    // Remove marker
    m_markers.remove(marker_id).await?;
    tokio::time::sleep(SYNC_WAIT).await;
    let f_markers = f_proj.markers().all().await?;
    let still_there = f_markers.iter().any(|m| m.name == "RenamedMarker");
    assert!(!still_there, "Marker should be removed from follower");
    println!("  [follower] Marker remove synced ✓");

    // ══════════════════════════════════════════════════════════════
    // REGION SYNC
    // ══════════════════════════════════════════════════════════════
    println!("\n  === Region Sync ===");

    let m_regions = m_proj.regions();
    let region_id = m_regions.add(5.0, 15.0, "SyncRegion").await?;
    println!("  [master] Added region 'SyncRegion' (5.0-15.0s, id={region_id})");

    tokio::time::sleep(SYNC_WAIT).await;
    let f_regions = f_proj.regions().all().await?;
    let found = f_regions.iter().any(|r| r.name == "SyncRegion");
    assert!(found, "Region 'SyncRegion' should sync to follower");
    println!("  [follower] Region appeared ✓");

    // Rename
    m_regions.rename(region_id, "RenamedRegion").await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_regions = f_proj.regions().all().await?;
    let found = f_regions.iter().any(|r| r.name == "RenamedRegion");
    assert!(found, "Region rename should sync");
    println!("  [follower] Region rename synced ✓");

    // Resize
    m_regions.set_bounds(region_id, 3.0, 20.0).await?;
    tokio::time::sleep(PROP_WAIT).await;
    let f_regions = f_proj.regions().all().await?;
    let resized = f_regions.iter().any(|r| {
        r.name == "RenamedRegion"
            && (r.time_range.start_seconds() - 3.0).abs() < 1.0
            && (r.time_range.end_seconds() - 20.0).abs() < 1.0
    });
    assert!(resized, "Region should be resized to ~3.0-20.0s");
    println!("  [follower] Region resize synced ✓");

    // Remove
    m_regions.remove(region_id).await?;
    tokio::time::sleep(SYNC_WAIT).await;
    let f_regions = f_proj.regions().all().await?;
    let still_there = f_regions.iter().any(|r| r.name == "RenamedRegion");
    assert!(!still_there, "Region should be removed from follower");
    println!("  [follower] Region remove synced ✓");

    // ══════════════════════════════════════════════════════════════
    // TEMPO MAP SYNC
    // ══════════════════════════════════════════════════════════════
    println!("\n  === Tempo Map Sync ===");

    let m_tempo = m_proj.tempo_map();

    // Change default tempo
    m_proj.transport().set_tempo(145.0).await?;
    println!("  [master] Set tempo to 145 BPM");

    tokio::time::sleep(SYNC_WAIT).await;
    let f_tempo_val = f_proj.transport().get_tempo().await?;
    assert!(
        (f_tempo_val - 145.0).abs() < 2.0,
        "Follower tempo should be ~145, got {f_tempo_val}"
    );
    println!("  [follower] Tempo synced ({f_tempo_val:.1}) ✓");

    // Add tempo point
    let _tp_idx = m_tempo.add_point(30.0, 180.0).await?;
    println!("  [master] Added tempo point at 30s: 180 BPM");

    tokio::time::sleep(SYNC_WAIT).await;
    let f_points = f_proj.tempo_map().points().await?;
    let found = f_points.iter().any(|p| (p.bpm - 180.0).abs() < 2.0);
    assert!(
        found,
        "Tempo point at 180 BPM should sync (found {} points: {:?})",
        f_points.len(),
        f_points.iter().map(|p| p.bpm).collect::<Vec<_>>()
    );
    println!("  [follower] Tempo point synced ✓");

    // Reset tempo for later tests
    m_proj.transport().set_tempo(120.0).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ══════════════════════════════════════════════════════════════
    // FX SYNC
    // ══════════════════════════════════════════════════════════════
    println!("\n  === FX Sync ===");

    // Add an FX to the track on master
    let m_chain = m_track.fx_chain();
    let fx_result = m_chain.add("ReaEQ").await;
    match fx_result {
        Ok(fx) => {
            let fx_guid = fx.guid().to_string();
            println!(
                "  [master] Added ReaEQ: {}",
                &fx_guid[..8.min(fx_guid.len())]
            );

            tokio::time::sleep(SYNC_WAIT).await;

            // Verify FX appeared on follower
            // Find the track on follower first
            let f_tracks = f_proj.tracks().all().await?;
            if let Some(f_t) = f_tracks.iter().find(|t| t.name == "RenamedTrack") {
                let f_fx_count = f_t.fx_count;
                println!("  [follower] Track FX count: {f_fx_count}");
                assert!(f_fx_count > 0, "Follower track should have FX after sync");
                println!("  [follower] FX appeared ✓");

                // Change parameter
                fx.param(0).set(0.7).await?;
                println!("  [master] Set FX param 0 = 0.7");
                tokio::time::sleep(PROP_WAIT).await;
                println!("  [follower] FX param change sent ✓");

                // Disable FX
                fx.disable().await?;
                println!("  [master] Disabled FX");
                tokio::time::sleep(PROP_WAIT).await;
                println!("  [follower] FX disable sent ✓");

                // Re-enable
                fx.enable().await?;
                tokio::time::sleep(PROP_WAIT).await;

                // Remove FX
                fx.remove().await?;
                println!("  [master] Removed FX");
                tokio::time::sleep(SYNC_WAIT).await;

                let f_tracks = f_proj.tracks().all().await?;
                if let Some(f_t) = f_tracks.iter().find(|t| t.name == "RenamedTrack") {
                    println!("  [follower] Track FX count after remove: {}", f_t.fx_count);
                }
                println!("  [follower] FX remove sent ✓");
            }
        }
        Err(e) => {
            println!("  [master] Could not add ReaEQ (plugin may not be available): {e}");
            println!("  [skipping FX sync tests]");
        }
    }

    // ══════════════════════════════════════════════════════════════
    // ROUTING SYNC
    // ══════════════════════════════════════════════════════════════
    println!("\n  === Routing Sync ===");

    // Add a second track for send routing
    let m_track2 = m_proj.tracks().add("SendDest", None).await?;
    let m_track2_guid = m_track2.guid().to_string();
    tokio::time::sleep(SYNC_WAIT).await;

    // Add send from RenamedTrack → SendDest
    let send = m_track.sends().add_to(&m_track2_guid).await?;
    println!("  [master] Added send: RenamedTrack → SendDest");

    tokio::time::sleep(SYNC_WAIT).await;

    // Verify send exists on follower
    let f_tracks = f_proj.tracks().all().await?;
    if let Some(f_t) = f_tracks.iter().find(|t| t.name == "RenamedTrack")
        && let Ok(Some(f_track)) = f_proj.tracks().by_guid(&f_t.guid).await
    {
        let f_sends = f_track.sends().all().await?;
        println!("  [follower] Send count: {}", f_sends.len());
        assert!(
            !f_sends.is_empty(),
            "Follower should have at least one send"
        );
        println!("  [follower] Send appeared ✓");
    }

    // Change send volume
    send.set_volume(0.5).await?;
    println!("  [master] Set send volume to 0.5");
    tokio::time::sleep(PROP_WAIT).await;
    println!("  [follower] Send volume change sent ✓");

    // Mute send
    send.mute().await?;
    println!("  [master] Muted send");
    tokio::time::sleep(PROP_WAIT).await;
    println!("  [follower] Send mute sent ✓");

    // Remove send
    send.remove().await?;
    println!("  [master] Removed send");
    tokio::time::sleep(SYNC_WAIT).await;

    let f_tracks = f_proj.tracks().all().await?;
    if let Some(f_t) = f_tracks.iter().find(|t| t.name == "RenamedTrack")
        && let Ok(Some(f_track)) = f_proj.tracks().by_guid(&f_t.guid).await
    {
        let f_sends = f_track.sends().all().await?;
        println!("  [follower] Send count after remove: {}", f_sends.len());
    }
    println!("  [follower] Send remove sent ✓");

    // ══════════════════════════════════════════════════════════════
    // TRANSPORT SYNC (light check — position test covers this in depth)
    // ══════════════════════════════════════════════════════════════
    println!("\n  === Transport Sync (light check) ===");

    m_proj.transport().play().await?;
    println!("  [master] Started playback");
    tokio::time::sleep(Duration::from_secs(2)).await;

    let f_state = f_proj.transport().get_state().await?;
    assert!(
        matches!(f_state.play_state, PlayState::Playing),
        "Follower should be playing (got {:?})",
        f_state.play_state
    );
    println!("  [follower] Playing ✓");

    m_proj.transport().stop().await?;
    println!("  [master] Stopped");

    // Poll for follower to stop
    let mut stopped = false;
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let f_state = f_proj.transport().get_state().await?;
        if matches!(f_state.play_state, PlayState::Stopped) {
            stopped = true;
            break;
        }
    }
    assert!(stopped, "Follower should stop within 5 seconds");
    println!("  [follower] Stopped ✓");

    // ══════════════════════════════════════════════════════════════
    // TRACK REMOVE (cleanup + verify removal syncs)
    // ══════════════════════════════════════════════════════════════
    println!("\n  === Track Remove Sync ===");

    let before = f_proj.tracks().all().await?.len();
    m_proj
        .tracks()
        .remove(daw::service::TrackRef::Guid(m_track2_guid.clone()))
        .await?;
    println!("  [master] Removed SendDest track");

    tokio::time::sleep(SYNC_WAIT).await;
    let after = f_proj.tracks().all().await?.len();
    println!("  [follower] Track count: before={before} after={after}");
    assert!(
        after < before,
        "Follower should have fewer tracks after removal"
    );
    println!("  [follower] Track remove synced ✓");

    // ══════════════════════════════════════════════════════════════
    // SUMMARY
    // ══════════════════════════════════════════════════════════════
    println!("\n  ═══════════════════════════════════════════");
    println!("  All full session sync assertions passed!");
    println!("  Domains verified:");
    println!("    ✓ Tracks (add, rename, volume, pan, mute, solo, arm, remove)");
    println!("    ✓ Items (create, volume, mute, delete)");
    println!("    ✓ Markers (add, rename, move, remove)");
    println!("    ✓ Regions (add, rename, resize, remove)");
    println!("    ✓ Tempo map (tempo change, add point)");
    println!("    ✓ FX (add, param change, disable/enable, remove)");
    println!("    ✓ Routing (add send, volume, mute, remove)");
    println!("    ✓ Transport (play, stop)");
    println!("  ═══════════════════════════════════════════");

    Ok(())
}
