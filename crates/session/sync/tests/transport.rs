//! One play button for everyone.

#![allow(clippy::float_cmp, clippy::unwrap_used)]

use session_sync::transport::{
    ClockSync, Command, Follower, LocalTransport, SETTLE_MS, SharedTransport, TransportMode,
    TransportSync,
};

fn shared(playing: bool, position: f64, at_ms: f64, seq: u64) -> SharedTransport {
    SharedTransport {
        mode: TransportMode::Shared,
        playing,
        position,
        at_ms,
        song: Some("washed".into()),
        seq,
        by: "alice".into(),
    }
}

fn local(playing: bool, position: f64) -> LocalTransport {
    LocalTransport {
        playing,
        position,
        song: Some("washed".into()),
    }
}

#[test]
fn it_round_trips() {
    let s = shared(true, 12.5, 1000.0, 7);
    assert_eq!(SharedTransport::decode(&s.encode()), Some(s));
}

#[test]
fn a_late_joiner_starts_where_the_song_is_now() {
    let mut f = Follower::default();
    // Alice pressed play at 10 s, two seconds ago.
    let cmds = f.step(&shared(true, 10.0, 0.0, 1), &local(false, 0.0), 2000.0);
    assert_eq!(cmds, vec![Command::Play { from: 12.0 }]);
}

#[test]
fn a_stop_stops_everyone_where_it_was_pressed() {
    let mut f = Follower::default();
    let cmds = f.step(&shared(false, 30.0, 5000.0, 2), &local(true, 30.02), 5010.0);
    assert_eq!(cmds, vec![Command::Stop { at: 30.0 }]);
}

#[test]
fn drift_is_corrected_but_jitter_is_not() {
    let mut f = Follower::default();
    let s = shared(true, 0.0, 0.0, 3);
    // First sight of this press: line up exactly.
    assert_eq!(
        f.step(&s, &local(true, 0.99), 1000.0),
        vec![Command::Seek { to: 1.0 }]
    );
    // 10 ms off: leave it alone.
    assert!(f.step(&s, &local(true, 2.01), 2000.0).is_empty());
    // 80 ms off: bring it back.
    assert_eq!(
        f.step(&s, &local(true, 2.92), 3000.0),
        vec![Command::Seek { to: 3.0 }]
    );
}

#[test]
fn a_different_song_is_opened_first() {
    let mut f = Follower::default();
    let here = LocalTransport {
        playing: false,
        position: 0.0,
        song: Some("praise".into()),
    };
    let cmds = f.step(&shared(true, 0.0, 0.0, 4), &here, 0.0);
    assert_eq!(
        cmds,
        vec![
            Command::SwitchSong("washed".into()),
            Command::Play { from: 0.0 }
        ]
    );
}

#[test]
fn independent_mode_asks_nothing() {
    let mut f = Follower::default();
    let s = SharedTransport {
        mode: TransportMode::Independent,
        ..shared(true, 0.0, 0.0, 5)
    };
    assert!(f.step(&s, &local(false, 40.0), 0.0).is_empty());
}

#[test]
fn the_clock_keeps_the_tightest_sample() {
    let mut c = ClockSync::default();
    // Host is 500 ms ahead. A slow, lopsided exchange, then a quick one.
    c.sample(0.0, 600.0, 200.0); // rtt 200, offset 500
    c.sample(1000.0, 1510.0, 1020.0); // rtt 20, offset 500
    c.sample(2000.0, 2600.0, 2100.0); // rtt 100, would say 550
    assert_eq!(c.round_trip(), Some(20.0));
    assert_eq!(c.shared(3000.0), 3500.0);
}

fn at(song: &str, playing: bool, position: f64) -> LocalTransport {
    LocalTransport {
        playing,
        position,
        song: Some(song.into()),
    }
}

/// Two peers in Shared mode, both stopped at 0 on `washed`.
fn pair() -> (TransportSync, TransportSync) {
    let (mut a, mut b) = (TransportSync::new("alice"), TransportSync::new("bob"));
    let together = a.set_mode(TransportMode::Shared, &at("washed", false, 0.0), 1000.0);
    assert!(b.remote(together));
    (a, b)
}

#[test]
fn a_press_here_is_not_undone_by_the_entry_it_replaced() {
    let (mut a, mut b) = pair();
    let stale = b.current().cloned().unwrap();
    // Bob presses play; his engine is playing.
    b.pressed(2000.0);
    assert_eq!(
        b.tick(&at("washed", true, 0.0), 2050.0),
        Default::default(),
        "not yet landed"
    );
    let press = b
        .tick(&at("washed", true, 0.15), 2000.0 + SETTLE_MS)
        .publish
        .expect("published");
    assert!(press.playing);
    // The presence still holds the old, stopped entry until his write
    // comes back: it must not stop him.
    assert!(!b.remote(stale));
    let later = b.tick(&at("washed", true, 0.5), 2500.0);
    assert!(later.commands.is_empty(), "{later:?}");
    // Alice hears of it and plays, from where he is now.
    assert!(a.remote(press));
    let tick = a.tick(&at("washed", false, 0.0), 2500.0);
    assert_eq!(tick.commands.len(), 1);
    let Command::Play { from } = tick.commands[0] else {
        panic!("{tick:?}")
    };
    assert!((from - 0.5).abs() < 1e-9, "{from}");
}

#[test]
fn a_command_is_given_time_to_land() {
    let (mut a, mut b) = pair();
    b.pressed(2000.0);
    let press = b.tick(&at("washed", true, 0.0), 2200.0).publish.unwrap();
    a.remote(press);
    assert!(matches!(
        a.tick(&at("washed", false, 0.0), 2300.0).commands[..],
        [Command::Play { .. }]
    ));
    // Still starting: not told again.
    assert!(
        a.tick(&at("washed", false, 0.0), 2350.0)
            .commands
            .is_empty()
    );
}

#[test]
fn a_seek_while_playing_moves_everyone() {
    let (mut a, mut b) = pair();
    b.pressed(2000.0);
    a.remote(b.tick(&at("washed", true, 0.0), 2200.0).publish.unwrap());
    a.tick(&at("washed", false, 0.0), 2300.0);
    // Bob clicks the ruler at 60 s.
    b.pressed(5000.0);
    let seek = b.tick(&at("washed", true, 60.0), 5200.0).publish.unwrap();
    a.remote(seek);
    let tick = a.tick(&at("washed", true, 3.1), 5300.0);
    let [Command::Seek { to }] = tick.commands[..] else {
        panic!("{tick:?}")
    };
    assert!((to - 60.1).abs() < 1e-9, "{to}");
}

#[test]
fn another_song_is_asked_for_once() {
    let (mut a, mut b) = pair();
    b.pressed(2000.0);
    a.remote(
        b.tick(&at("holy-forever", false, 0.0), 2200.0)
            .publish
            .unwrap(),
    );
    assert_eq!(
        a.tick(&at("washed", false, 0.0), 2300.0).commands,
        vec![Command::SwitchSong("holy-forever".into())]
    );
    // The switch takes a while (the audio device moves): not asked again,
    // and not mistaken for a press here.
    let waiting = a.tick(&at("washed", false, 0.0), 2600.0);
    assert_eq!(waiting, Default::default());
    // There: nothing more to do.
    assert!(
        a.tick(&at("holy-forever", false, 0.0), 3000.0)
            .commands
            .is_empty()
    );
}

#[test]
fn two_presses_at_once_settle_on_the_same_one() {
    let (mut a, mut b) = pair();
    a.pressed(2000.0);
    b.pressed(2000.0);
    let from_a = a.tick(&at("washed", true, 10.0), 2200.0).publish.unwrap();
    let from_b = b.tick(&at("washed", false, 20.0), 2200.0).publish.unwrap();
    a.remote(from_b.clone());
    b.remote(from_a.clone());
    assert_eq!(a.current(), b.current(), "both keep the same winner");
}

#[test]
fn playing_apart_nobody_is_moved() {
    let (mut a, _) = pair();
    let apart = a.set_mode(
        TransportMode::Independent,
        &at("washed", false, 0.0),
        3000.0,
    );
    let mut b = TransportSync::new("bob");
    b.remote(apart);
    b.pressed(3100.0);
    assert_eq!(b.tick(&at("washed", true, 5.0), 3500.0), Default::default());
}

#[test]
fn a_leaders_stamped_playhead_round_trips() {
    use session_sync::transport::SyncPosition;
    let p = SyncPosition {
        song: Some("Washed".into()),
        position: daw_transport_sync::Position {
            host_micros: 123_456_789.25,
            playhead_seconds: 61.000_020_8,
            playrate: 1.000_08,
            is_playing: true,
        },
    };
    assert_eq!(SyncPosition::decode(&p.encode()), Some(p));
}
