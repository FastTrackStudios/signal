//! Unit tests for the Link engine using mock callbacks.
//!
//! These tests exercise the engine's puppet/master logic without a running DAW.
//! The `LinkCallbacks` trait is host-agnostic by design, so we provide a
//! `MockCallbacks` that records all mutations for assertion.

use daw_link::{Engine, EngineConfig, LinkCallbacks, LinkState, Mode};
use std::cell::RefCell;

fn default_state() -> LinkState {
    LinkState {
        is_playing: false,
        position_seconds: 0.0,
        beat_position: 0.0,
        qn_position: 0.0,
        tempo_bpm: 120.0,
        timesig_num: 4,
        timesig_denom: 4,
        output_latency: 0.0,
        is_looping: false,
        playrate: 1.0,
        loop_range: None,
        loop_range_beats: None,
    }
}

/// Records all host mutations the engine makes during a tick.
struct MockCallbacks {
    state: LinkState,
    log: RefCell<Vec<String>>,
}

impl MockCallbacks {
    fn new(state: LinkState) -> Self {
        Self {
            state,
            log: RefCell::new(Vec::new()),
        }
    }

    fn log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }

    fn has_action(&self, prefix: &str) -> bool {
        self.log.borrow().iter().any(|s| s.starts_with(prefix))
    }
}

impl LinkCallbacks for MockCallbacks {
    fn capture_state(&self) -> LinkState {
        self.state.clone()
    }

    fn play(&self) {
        self.log.borrow_mut().push("play".into());
    }

    fn stop(&self) {
        self.log.borrow_mut().push("stop".into());
    }

    fn set_cursor_position(&self, seconds: f64) {
        self.log
            .borrow_mut()
            .push(format!("set_cursor_position:{seconds:.3}"));
    }

    fn set_tempo(&self, bpm: f64) {
        self.log.borrow_mut().push(format!("set_tempo:{bpm:.1}"));
    }

    fn get_loop_range(&self) -> Option<(f64, f64)> {
        self.state.loop_range
    }

    fn nudge_playrate_up(&self) {
        self.log.borrow_mut().push("nudge_playrate_up".into());
    }

    fn nudge_playrate_down(&self) {
        self.log.borrow_mut().push("nudge_playrate_down".into());
    }

    fn reset_playrate(&self) {
        self.log.borrow_mut().push("reset_playrate".into());
    }

    fn go_to_region(&self, region_id: i32) {
        self.log
            .borrow_mut()
            .push(format!("go_to_region:{region_id}"));
    }

    fn add_project_marker(&self, is_region: bool, start: f64, end: f64, name: &str) -> i32 {
        self.log
            .borrow_mut()
            .push(format!("add_marker:{is_region}:{start:.1}:{end:.1}:{name}"));
        1
    }

    fn remove_project_marker(&self, marker_id: i32) {
        self.log
            .borrow_mut()
            .push(format!("remove_marker:{marker_id}"));
    }

    fn set_tempo_at_position(
        &self,
        position: f64,
        bpm: f64,
        _timesig_num: i32,
        _timesig_denom: i32,
    ) {
        self.log
            .borrow_mut()
            .push(format!("set_tempo_at_pos:{position:.1}:{bpm:.1}"));
    }

    fn get_next_measure_position(&self) -> f64 {
        // Return the next measure at 2.0 seconds (120 BPM, 4/4 = 2s per measure)
        2.0
    }

    fn create_midi_item(&self, track_index: usize, start: f64, end: f64) {
        self.log.borrow_mut().push(format!(
            "create_midi_item:{track_index}:{start:.1}:{end:.1}"
        ));
    }

    fn get_project_length(&self) -> f64 {
        300.0
    }

    fn time_to_beats(&self, time_seconds: f64) -> (f64, i32) {
        let bps = self.state.tempo_bpm / 60.0;
        let total_beats = time_seconds * bps;
        let measure = (total_beats / self.state.timesig_num as f64).floor() as i32;
        let beat_in_measure = total_beats - (measure as f64 * self.state.timesig_num as f64);
        (beat_in_measure, measure)
    }

    fn beats_to_time(&self, beat: f64, measure: i32) -> f64 {
        let bps = self.state.tempo_bpm / 60.0;
        let total_beats = (measure as f64 * self.state.timesig_num as f64) + beat;
        total_beats / bps
    }

    fn clear_link_dummy_objects(&self) {
        self.log.borrow_mut().push("clear_link_dummy".into());
    }
}

// ── Engine lifecycle tests ──────────────────────────────────────────────────

#[test]
fn engine_starts_disabled() {
    let engine = Engine::new(120.0, EngineConfig::default());
    assert_eq!(engine.mode(), Mode::Off);
}

#[test]
fn engine_mode_switching() {
    let mut engine = Engine::new(120.0, EngineConfig::default());

    engine.set_mode(Mode::Puppet);
    assert_eq!(engine.mode(), Mode::Puppet);

    engine.set_mode(Mode::Master);
    assert_eq!(engine.mode(), Mode::Master);

    engine.set_mode(Mode::Off);
    assert_eq!(engine.mode(), Mode::Off);
}

#[test]
fn engine_tick_noop_when_off() {
    let mut engine = Engine::new(120.0, EngineConfig::default());
    let mock = MockCallbacks::new(default_state());

    // Ticking with mode=Off should not call any callbacks
    engine.tick(&mock);
    assert!(mock.log().is_empty(), "off mode should not produce actions");
}

#[test]
fn engine_enable_disable_link() {
    let engine = Engine::new(120.0, EngineConfig::default());

    engine.set_enabled(true);
    assert!(engine.is_enabled());

    engine.set_enabled(false);
    assert!(!engine.is_enabled());
}

// ── Puppet mode tests ───────────────────────────────────────────────────────

#[test]
fn puppet_stops_reaper_when_host_playing_but_link_not() {
    let mut engine = Engine::new(
        120.0,
        EngineConfig {
            mode: Mode::Puppet,
            ..Default::default()
        },
    );
    engine.set_enabled(true);

    // Host is playing but Link session is not — puppet should stop host
    let mock = MockCallbacks::new(LinkState {
        is_playing: true,
        tempo_bpm: 120.0,
        ..default_state()
    });

    engine.tick(&mock);
    assert!(
        mock.has_action("stop"),
        "puppet should stop host when Link is not playing but host is: {:?}",
        mock.log()
    );
}

// ── Master mode tests ───────────────────────────────────────────────────────

#[test]
fn master_pushes_tempo_to_link() {
    let mut engine = Engine::new(
        120.0,
        EngineConfig {
            mode: Mode::Master,
            ..Default::default()
        },
    );
    engine.set_enabled(true);

    let mock = MockCallbacks::new(LinkState {
        is_playing: true,
        tempo_bpm: 140.0, // host at 140, link session at 120
        qn_position: 1.0,
        ..default_state()
    });

    // Tick several times — master should push 140 BPM to the session
    for _ in 0..5 {
        engine.tick(&mock);
    }

    // The session tempo should have been set to 140
    let session_tempo = engine.session_tempo();
    assert!(
        (session_tempo - 140.0).abs() < 1.0,
        "master should push host tempo to Link session, got {session_tempo}"
    );
}

#[test]
fn master_does_not_modify_host() {
    let mut engine = Engine::new(
        120.0,
        EngineConfig {
            mode: Mode::Master,
            ..Default::default()
        },
    );
    engine.set_enabled(true);

    let mock = MockCallbacks::new(LinkState {
        is_playing: true,
        tempo_bpm: 120.0,
        qn_position: 2.0,
        ..default_state()
    });

    engine.tick(&mock);

    // Master mode should not call play/stop/set_tempo on the host
    let actions = mock.log();
    let host_mutations: Vec<_> = actions
        .iter()
        .filter(|a| {
            a.starts_with("play")
                || a.starts_with("stop")
                || a.starts_with("set_tempo")
                || a.starts_with("nudge")
        })
        .collect();
    assert!(
        host_mutations.is_empty(),
        "master should not mutate host state, got: {:?}",
        host_mutations
    );
}

// ── Config tests ────────────────────────────────────────────────────────────

#[test]
fn engine_config_update() {
    let mut engine = Engine::new(120.0, EngineConfig::default());

    let new_config = EngineConfig {
        mode: Mode::Puppet,
        push_local_tempo: false,
        start_stop_sync: false,
        phase_tolerance: Some(0.01),
    };
    engine.set_config(new_config);

    // Mode is updated via set_config
    assert_eq!(engine.mode(), Mode::Puppet);
}

#[test]
fn initial_session_tempo_matches_constructor() {
    let engine = Engine::new(135.5, EngineConfig::default());
    let tempo = engine.session_tempo();
    assert!(
        (tempo - 135.5).abs() < 0.01,
        "session tempo should match initial: {tempo}"
    );
}
