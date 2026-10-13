//! Shared transport: one play button for everyone.
//!
//! Two modes, chosen per session:
//!
//! - **Independent** — the doc is shared, the transport is not. Everyone
//!   runs their own engine and plays what they need to hear; the others'
//!   play cursors are ghosts (see `presence::PlayState`).
//! - **Shared** — one transport. Whoever presses play, everyone plays,
//!   from the same place, at the same moment.
//!
//! The shared transport is ONE presence entry, [`KEY`], that anyone may
//! write: last writer wins, which is what a play button pressed by two
//! people at once should do. It says where the song was at a moment of
//! the *shared clock*, so a peer that hears about it late still starts in
//! the right place — [`ClockSync`] maps this machine's clock onto it.
//! Each peer's [`Follower`] turns the entry into what its own engine must
//! do, and keeps it there: a peer that drifts is nudged back.
//!
//! [`TransportSync`] is the whole of it for one peer — what an app drives:
//! it is told when someone pressed something HERE ([`TransportSync::pressed`])
//! and what the presence holds ([`TransportSync::remote`]), and each tick
//! says what to publish and what the engine must do. What it guarantees:
//!
//! - **A press here wins until a newer one arrives.** Entries are ordered
//!   by `(seq, by)`; an older one — including the entry this peer's own
//!   press replaced, still in the presence until its write comes back — is
//!   never adopted, so it cannot undo the press.
//! - **A press is what it did.** The entry published is the engine's state
//!   once the press has landed (a moment later), not a guess at what the
//!   button meant — play, stop, a seek, a jump home, another song.
//! - **Commands settle.** After the engine is told something, it is given
//!   a moment to do it before it is judged again — no second Play while
//!   the first is still starting, and a song switch (which moves the audio
//!   device) is asked for once, not every tick.

use loro::LoroValue;

/// The presence key the shared transport lives under.
pub const KEY: &str = "session/transport";

/// How far a following engine may drift before it is corrected, seconds.
pub const DRIFT_TOLERANCE: f64 = 0.030;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportMode {
    #[default]
    Independent,
    Shared,
}

/// The session's transport, as last set by anyone.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedTransport {
    pub mode: TransportMode,
    pub playing: bool,
    /// Seconds into `song` at `at_ms`.
    pub position: f64,
    /// Shared-clock milliseconds.
    pub at_ms: f64,
    /// Which song of the setlist (its id); `None` for a lone session.
    pub song: Option<String>,
    /// Bumped by every write, so a follower acts on each press once.
    pub seq: u64,
    /// Who pressed it — shown beside the transport ("Alice started").
    pub by: String,
}

impl SharedTransport {
    /// Where the song is at `now_ms` (shared clock).
    #[must_use]
    pub fn position_at(&self, now_ms: f64) -> f64 {
        if self.playing {
            ((now_ms - self.at_ms) / 1000.0) + self.position
        } else {
            self.position
        }
    }

    #[must_use]
    pub fn encode(&self) -> LoroValue {
        let m: std::collections::HashMap<String, LoroValue> = [
            (
                "mode",
                LoroValue::from(if self.mode == TransportMode::Shared {
                    "shared"
                } else {
                    "independent"
                }),
            ),
            ("playing", LoroValue::Bool(self.playing)),
            ("position", LoroValue::Double(self.position)),
            ("at", LoroValue::Double(self.at_ms)),
            (
                "song",
                self.song
                    .as_deref()
                    .map_or(LoroValue::Null, LoroValue::from),
            ),
            (
                "seq",
                LoroValue::I64(i64::try_from(self.seq).unwrap_or(i64::MAX)),
            ),
            ("by", LoroValue::from(self.by.as_str())),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        LoroValue::from(m)
    }

    #[must_use]
    pub fn decode(v: &LoroValue) -> Option<Self> {
        let LoroValue::Map(m) = v else { return None };
        let f64_of = |k: &str| match m.get(k)? {
            LoroValue::Double(d) => Some(*d),
            LoroValue::I64(i) => i32::try_from(*i).ok().map(f64::from),
            _ => None,
        };
        let str_of = |k: &str| match m.get(k)? {
            LoroValue::String(s) => Some(s.to_string()),
            _ => None,
        };
        Some(Self {
            mode: match str_of("mode")?.as_str() {
                "shared" => TransportMode::Shared,
                _ => TransportMode::Independent,
            },
            playing: matches!(m.get("playing")?, LoroValue::Bool(true)),
            position: f64_of("position")?,
            at_ms: f64_of("at")?,
            song: str_of("song"),
            seq: match m.get("seq")? {
                LoroValue::I64(i) => u64::try_from(*i).ok()?,
                _ => return None,
            },
            by: str_of("by").unwrap_or_default(),
        })
    }
}

/// What this peer's engine is doing right now.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalTransport {
    pub playing: bool,
    pub position: f64,
    pub song: Option<String>,
}

/// What the follower asks of the local engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Open this song first (then the next step applies to it).
    SwitchSong(String),
    /// Start playing from here.
    Play { from: f64 },
    /// Stop, and rest here.
    Stop { at: f64 },
    /// Keep playing, from here — a correction.
    Seek { to: f64 },
}

/// Keeps one engine on the shared transport.
#[derive(Debug, Clone, Default)]
pub struct Follower {
    seen: Option<u64>,
}

impl Follower {
    /// What to do, given the shared transport and this engine's state at
    /// `now_ms` (shared clock). Call on every change of either, and
    /// periodically while playing to correct drift.
    pub fn step(
        &mut self,
        shared: &SharedTransport,
        local: &LocalTransport,
        now_ms: f64,
    ) -> Vec<Command> {
        if shared.mode != TransportMode::Shared {
            return Vec::new();
        }
        let fresh = self.seen != Some(shared.seq);
        self.seen = Some(shared.seq);
        let mut out = Vec::new();
        if let Some(song) = &shared.song
            && local.song.as_ref() != Some(song)
        {
            out.push(Command::SwitchSong(song.clone()));
        }
        let target = shared.position_at(now_ms);
        match (shared.playing, local.playing) {
            (true, false) => out.push(Command::Play { from: target }),
            (false, true) => out.push(Command::Stop { at: target }),
            (true, true) => {
                if fresh || (local.position - target).abs() > DRIFT_TOLERANCE {
                    out.push(Command::Seek { to: target });
                }
            }
            (false, false) => {
                if fresh && (local.position - target).abs() > f64::EPSILON {
                    out.push(Command::Stop { at: target });
                }
            }
        }
        out
    }
}

/// This machine's clock mapped onto the shared one (the host's).
///
/// NTP's trick: send at `t0` (local), the host answers with its time `ts`,
/// the answer arrives at `t1` (local). If the trip was symmetric, the host
/// read its clock at local `(t0 + t1) / 2`. The sample with the shortest
/// round trip has the least room for asymmetry, so it is the one kept.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClockSync {
    best: Option<(f64, f64)>, // (round trip, offset)
}

impl ClockSync {
    /// Record one exchange; all times in milliseconds.
    pub fn sample(&mut self, t0: f64, host_ms: f64, t1: f64) {
        let rtt = t1 - t0;
        if rtt < 0.0 {
            return;
        }
        let offset = host_ms - t0.midpoint(t1);
        if self.best.is_none_or(|(best, _)| rtt < best) {
            self.best = Some((rtt, offset));
        }
    }

    /// Shared-clock time for a local time.
    #[must_use]
    pub fn shared(&self, local_ms: f64) -> f64 {
        local_ms + self.best.map_or(0.0, |(_, offset)| offset)
    }

    /// The round trip of the sample in use, if any — how much to trust it.
    #[must_use]
    pub fn round_trip(&self) -> Option<f64> {
        self.best.map(|(rtt, _)| rtt)
    }
}

/// How long a press here, or a command to the engine, is given to land
/// before the engine's state is read as settled, milliseconds.
pub const SETTLE_MS: f64 = 150.0;

/// How long a song switch the follower asked for is given before it is
/// asked for again, milliseconds (switching moves the audio device).
pub const SWITCH_MS: f64 = 3000.0;

/// One peer's end of the shared transport.
#[derive(Debug, Clone)]
pub struct TransportSync {
    me: String,
    current: Option<SharedTransport>,
    follower: Follower,
    /// A press here, not yet published: when it happened.
    pressed_at: Option<f64>,
    /// The engine was just told something: leave it alone until then.
    quiet_until: f64,
    /// The song switch asked for, and when.
    switching: Option<(String, f64)>,
}

/// What one tick asks of the app.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tick {
    /// Write this under [`KEY`].
    pub publish: Option<SharedTransport>,
    /// Do these to the engine, in order.
    pub commands: Vec<Command>,
}

impl TransportSync {
    #[must_use]
    pub fn new(me: impl Into<String>) -> Self {
        Self {
            me: me.into(),
            current: None,
            follower: Follower::default(),
            pressed_at: None,
            quiet_until: f64::NEG_INFINITY,
            switching: None,
        }
    }

    /// The session's mode, as last set by anyone.
    #[must_use]
    pub fn mode(&self) -> TransportMode {
        self.current
            .as_ref()
            .map_or(TransportMode::Independent, |c| c.mode)
    }

    /// The entry in force.
    #[must_use]
    pub const fn current(&self) -> Option<&SharedTransport> {
        self.current.as_ref()
    }

    /// Someone pressed something on this transport (play, stop, a seek,
    /// home, end, another song). Published once it has landed.
    pub fn pressed(&mut self, now_ms: f64) {
        if self.mode() == TransportMode::Shared {
            self.pressed_at = Some(now_ms);
        }
    }

    /// What the presence holds under [`KEY`]. Adopted only when newer than
    /// what is in force; returns whether it was.
    pub fn remote(&mut self, entry: SharedTransport) -> bool {
        let newer = self
            .current
            .as_ref()
            .is_none_or(|c| (entry.seq, entry.by.as_str()) > (c.seq, c.by.as_str()));
        if newer {
            self.current = Some(entry);
        }
        newer
    }

    /// Switch everyone between playing apart and together, from where this
    /// engine is. Returns the entry to publish.
    pub fn set_mode(
        &mut self,
        mode: TransportMode,
        local: &LocalTransport,
        now_ms: f64,
    ) -> SharedTransport {
        let entry = self.entry(mode, local, now_ms);
        self.pressed_at = None;
        entry
    }

    /// Once per tick, with this engine's state.
    pub fn tick(&mut self, local: &LocalTransport, now_ms: f64) -> Tick {
        if self.mode() != TransportMode::Shared {
            self.pressed_at = None;
            return Tick::default();
        }
        // A press here, landed: it is the transport now.
        if let Some(at) = self.pressed_at {
            if now_ms - at < SETTLE_MS {
                return Tick::default();
            }
            self.pressed_at = None;
            let entry = self.entry(TransportMode::Shared, local, now_ms);
            self.switching = None;
            return Tick {
                publish: Some(entry),
                commands: Vec::new(),
            };
        }
        if now_ms < self.quiet_until {
            return Tick::default();
        }
        let Some(current) = self.current.clone() else {
            return Tick::default();
        };
        let mut commands = self.follower.step(&current, local, now_ms);
        // A switch already asked for is not asked for again (until it has
        // had its time); the rest waits for the song to be there.
        if let Some(Command::SwitchSong(song)) = commands.first() {
            let asked = self
                .switching
                .as_ref()
                .is_some_and(|(s, at)| s == song && now_ms - at < SWITCH_MS);
            if asked {
                return Tick::default();
            }
            self.switching = Some((song.clone(), now_ms));
            commands.truncate(1);
        } else {
            self.switching = None;
        }
        if !commands.is_empty() {
            self.quiet_until = now_ms + SETTLE_MS;
        }
        Tick {
            publish: None,
            commands,
        }
    }

    /// A new entry from this engine's state, in force from now.
    fn entry(
        &mut self,
        mode: TransportMode,
        local: &LocalTransport,
        now_ms: f64,
    ) -> SharedTransport {
        let floor = self.current.as_ref().map_or(0, |c| c.seq.saturating_add(1));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let clock = (now_ms.max(0.0) as u64).saturating_mul(1000);
        let entry = SharedTransport {
            mode,
            playing: local.playing,
            position: local.position,
            at_ms: now_ms,
            song: local.song.clone(),
            seq: floor.max(clock),
            by: self.me.clone(),
        };
        // Its own press is not news to this engine.
        self.follower.seen = Some(entry.seq);
        self.quiet_until = now_ms + SETTLE_MS;
        self.current = Some(entry.clone());
        entry
    }
}

/// The presence key a peer publishes its clock-stamped playhead under
/// while it leads the shared transport.
#[must_use]
pub fn sync_key(peer: &str) -> String {
    format!("{peer}/sync")
}

/// A leader's playhead for followers to lock to: where it was, when (in
/// the session's shared clock, µs), how fast, on which song. Published
/// every tick while it leads — a projection is exact at the stated rate,
/// but the leader's own device clock wanders against the shared one, so
/// followers want fresh stamps, not just the last change.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncPosition {
    pub song: Option<String>,
    pub position: daw_transport_sync::Position,
}

impl SyncPosition {
    #[must_use]
    pub fn encode(&self) -> LoroValue {
        let p = self.position;
        let m: std::collections::HashMap<String, LoroValue> = [
            ("at", LoroValue::Double(p.host_micros)),
            ("playhead", LoroValue::Double(p.playhead_seconds)),
            ("rate", LoroValue::Double(p.playrate)),
            ("playing", LoroValue::Bool(p.is_playing)),
            (
                "song",
                self.song
                    .as_deref()
                    .map_or(LoroValue::Null, LoroValue::from),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        LoroValue::from(m)
    }

    #[must_use]
    pub fn decode(v: &LoroValue) -> Option<Self> {
        let LoroValue::Map(m) = v else { return None };
        let f = |k: &str| match m.get(k)? {
            LoroValue::Double(d) => Some(*d),
            _ => None,
        };
        Some(Self {
            song: match m.get("song") {
                Some(LoroValue::String(s)) => Some(s.to_string()),
                _ => None,
            },
            position: daw_transport_sync::Position {
                host_micros: f("at")?,
                playhead_seconds: f("playhead")?,
                playrate: f("rate")?,
                is_playing: matches!(m.get("playing")?, LoroValue::Bool(true)),
            },
        })
    }
}
