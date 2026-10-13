//! A host and a joiner over a real vox connection (in-process link).

#![cfg(feature = "net")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::panic
)]

use std::time::Duration;

use session_sync::net::{CollabHost, CollabPeer, PresenceSink, session_id};
use session_sync::presence::{self, PeerState};
use session_sync::{ItemState, ORIGIN_LOCAL, SessionDoc, SessionModel, TrackState};

fn song() -> SessionModel {
    let mut m = SessionModel {
        tracks: vec![TrackState {
            guid: "bass".into(),
            name: "Bass".into(),
            volume: 1.0,
            ..TrackState::default()
        }],
        chart: "Song\n72bpm 4/4 #E\n".into(),
        ..SessionModel::default()
    };
    m.items.insert(
        "i1".into(),
        ItemState {
            track: "bass".into(),
            position: 1.0,
            length: 2.0,
            ..ItemState::default()
        },
    );
    m
}

async fn until(mut ok: impl FnMut() -> bool) {
    for _ in 0..200 {
        if ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_joiner_gets_the_session_and_edits_flow_both_ways() {
    let host_doc = SessionDoc::new();
    host_doc.write(&song(), ORIGIN_LOCAL).unwrap();
    let id = session_id("{PROJECT-GUID}");
    let host = CollabHost::new(id, &host_doc);

    let router = host.mount(architect::LayerRouter::new());
    let server = architect::LocalServer::serve(router, architect::Scope::new());
    let caller = server.caller().await.unwrap();

    let (peer_doc, peer_presence, mut synced, mut driver) = CollabPeer::new(id).into_parts();
    tokio::spawn(async move {
        let sync = crdt::sync::DocSyncClient::new(caller.clone());
        let presence = crdt::sync::DocPresenceClient::new(caller);
        let _ = tokio::join!(synced.run(&sync), driver.run(&presence));
    });

    // The joiner receives the host's session.
    until(|| peer_doc.read() == song()).await;

    // The joiner edits; the host sees it.
    let mut m = peer_doc.read();
    m.items.get_mut("i1").unwrap().position = 4.0;
    peer_doc.write(&m, ORIGIN_LOCAL).unwrap();
    until(|| host_doc.read().items["i1"].position == 4.0).await;

    // The host edits; the joiner sees it.
    let mut m = host_doc.read();
    m.tracks[0].name = "Bass DI".into();
    host_doc.write(&m, ORIGIN_LOCAL).unwrap();
    until(|| peer_doc.read().tracks[0].name == "Bass DI").await;

    // Presence: the joiner says who it is; the host hears.
    let me = PeerState {
        name: "Joiner".into(),
        view: "arrangement".into(),
        ..PeerState::default()
    };
    PresenceSink::set(
        &peer_presence,
        &presence::key("joiner", presence::STATE),
        me.encode(),
    );
    until(|| {
        host.states()
            .get(&presence::key("joiner", presence::STATE))
            .and_then(PeerState::decode)
            .is_some_and(|s| s.name == "Joiner")
    })
    .await;
    assert_eq!(host.peers(), 1);

    // The host is a participant too: its own presence reaches the joiner.
    let host_me = PeerState {
        name: "Host".into(),
        edit_cursor: Some(3.0),
        ..PeerState::default()
    };
    PresenceSink::set(
        &host,
        &presence::key("host", presence::STATE),
        host_me.encode(),
    );
    until(|| {
        peer_presence
            .states()
            .get(&presence::key("host", presence::STATE))
            .and_then(PeerState::decode)
            .is_some_and(|s| s.name == "Host" && s.edit_cursor == Some(3.0))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_whole_set_is_shared_and_people_can_be_on_different_songs() {
    use session_sync::net::{SetHost, SetPeer, dial_fixed, song_id};
    let set = "Worship Set";
    let (washed, praise) = (SessionDoc::new(), SessionDoc::new());
    washed.write(&song(), ORIGIN_LOCAL).unwrap();
    let mut p = song();
    p.chart = "Praise\n".into();
    praise.write(&p, ORIGIN_LOCAL).unwrap();

    let host = SetHost::new(session_id(set));
    host.add(song_id(set, "Washed"), &washed);
    host.add(song_id(set, "Praise"), &praise);
    let server = architect::LocalServer::serve(
        host.mount(architect::LayerRouter::new()),
        architect::Scope::new(),
    );

    // The joiner replicates both songs over one link each.
    let sync = || async {
        server
            .establish::<crdt::sync::DocSyncClient>()
            .await
            .unwrap()
    };
    let peer_washed = SetPeer::sync_song(song_id(set, "Washed"), dial_fixed(sync().await));
    let peer_praise = SetPeer::sync_song(song_id(set, "Praise"), dial_fixed(sync().await));
    until(|| peer_washed.read() == song()).await;
    until(|| peer_praise.read().chart == "Praise\n").await;

    // An edit on one song reaches the host's doc for that song only.
    let mut m = peer_praise.read();
    m.items.get_mut("i1").unwrap().position = 9.0;
    peer_praise.write(&m, ORIGIN_LOCAL).unwrap();
    until(|| praise.read().items["i1"].position == 9.0).await;
    assert_eq!(washed.read().items["i1"].position, 1.0);

    // One presence channel for the set: the host (as its own peer) sees
    // the joiner, who is on another song.
    let mut joiner = SetPeer::new(host.id());
    joiner.run_presence(dial_fixed(server.establish().await.unwrap()));
    let me = PeerState {
        name: "Alice".into(),
        song: Some("Praise".into()),
        ..PeerState::default()
    };
    PresenceSink::set(
        joiner.presence(),
        &presence::key("alice", presence::STATE),
        me.encode(),
    );
    let host_view = host.own_presence().await.unwrap();
    until(|| {
        PresenceSink::states(&host_view)
            .get(&presence::key("alice", presence::STATE))
            .and_then(PeerState::decode)
            .is_some_and(|s| s.song.as_deref() == Some("Praise"))
    })
    .await;
}

/// A joiner finds the host's clock: in one process the two clocks are the
/// same one, so the offset it estimates is (near) zero, and it is known
/// within a few pings.
#[tokio::test]
async fn a_joiner_finds_the_shared_clock() {
    use session_sync::clock::{SessionClockClient, SharedClock};
    let host = session_sync::net::SetHost::new(session_sync::net::session_id("Worship Set"));
    let server = architect::LocalServer::serve(
        host.mount(architect::LayerRouter::new()),
        architect::Scope::new(),
    );
    let client: SessionClockClient = server.establish::<SessionClockClient>().await.unwrap();
    let clock = SharedClock::follow(client);
    let mut offset = None;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        offset = clock.offset_micros();
        if offset.is_some() && clock.round_trip_micros().is_some() {
            break;
        }
    }
    let offset = offset.expect("an estimate after a few pings");
    let rtt = clock.round_trip_micros().unwrap();
    assert!(
        offset.abs() < rtt.max(1_000.0),
        "offset {offset} µs, round trip {rtt} µs"
    );
    assert!(
        SharedClock::owned().offset_micros() == Some(0.0),
        "the host's own clock is the shared one"
    );
}
