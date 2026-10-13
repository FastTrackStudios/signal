//! Who else is here and what they are doing: ephemeral, never saved.
//!
//! Presence rides a Loro `EphemeralStore` (architect's `DocPresence`
//! channel), which is last-writer-wins per key with a timeout. Each peer
//! owns three keys, split by how often they change — the lesson of
//! Canva's multiplayer pointers is that the pointer is almost all of the
//! traffic, so it must not drag everything else along with it:
//!
//! | key               | what                                   | rate                 |
//! |-------------------|----------------------------------------|----------------------|
//! | `<peer>/state`    | name, colour, view, cursor, selections | on change            |
//! | `<peer>/pointer`  | the mouse, in timeline coordinates     | on move, ≤ ~30 Hz    |
//! | `<peer>/play`     | playing, where, and when that was      | on transport change  |
//!
//! Positions are in *timeline* coordinates (seconds, track guid) — never
//! pixels — so a pointer lands on the same beat of the same track at any
//! zoom or scroll. The play cursor is not streamed: `play` says where the
//! peer was at a moment, and the receiver extrapolates. Pointer samples
//! are drawn a little in the past and interpolated ([`PointerTrail`]),
//! which turns a 30 Hz stream into smooth motion.

use std::collections::{BTreeMap, VecDeque};

use loro::LoroValue;

/// Suffixes of a peer's presence keys.
pub const STATE: &str = "state";
pub const POINTER: &str = "pointer";
pub const PLAY: &str = "play";

/// The key a peer publishes `part` under.
#[must_use]
pub fn key(peer: &str, part: &str) -> String {
    format!("{peer}/{part}")
}

/// Split a presence key into `(peer, part)`.
#[must_use]
pub fn split_key(key: &str) -> Option<(&str, &str)> {
    key.rsplit_once('/')
}

/// The slow-changing part of a peer's presence.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PeerState {
    pub name: String,
    /// `0xRRGGBB`, the colour their ghosts are drawn in.
    pub color: u32,
    /// The song they have open (its setlist id), if any.
    pub song: Option<String>,
    /// Which view: `arrangement`, `mixer`, `chart`, `overview`, …
    pub view: String,
    /// The edit cursor, seconds.
    pub edit_cursor: Option<f64>,
    /// A time selection, seconds.
    pub time_selection: Option<(f64, f64)>,
    pub selected_tracks: Vec<String>,
    pub selected_items: Vec<String>,
    /// Their caret in the chart editor: anchor and head, as encoded Loro
    /// cursors (`SessionDoc::chart_cursor`), so they stay on the right
    /// character while others type.
    pub chart_caret: Option<(Vec<u8>, Vec<u8>)>,
}

/// Where the mouse is.
#[derive(Debug, Clone, PartialEq)]
pub enum Pointer {
    /// Over the arrangement: a time, and the track row under it (none
    /// over the ruler).
    ///
    /// `y` is where in that track's row, 0 (top) to 1 (bottom) — the
    /// pointer is wherever the hand is, not snapped to the row's middle —
    /// or, with no track (over the ruler), 0..1 down the ruler.
    Timeline {
        at: f64,
        track: Option<String>,
        y: f64,
    },
    /// Over the chart: a position on its page, 0..1 each way.
    Chart { x: f64, y: f64 },
    /// Anywhere else in the window: a named part of it (`chart`,
    /// `lyrics`, `panels`, `editor`, … or `window` for the whole) and the
    /// position within it, 0..1 each way — so it lands on the same place
    /// of the same panel whatever size each person's window is.
    Region { region: String, x: f64, y: f64 },
    /// Over a panel's CONTENT: anchored to what is under it rather than to
    /// where it is on screen, so it lands on the same thing in a view
    /// laid out, zoomed or scrolled differently. The panel decides what
    /// `key`, `u` and `v` mean — the chart: a measure, how far through it
    /// and how high against its staff; the mixer: a track's strip and
    /// where on it; the progress bar: the song, and a time.
    Anchor {
        panel: String,
        key: String,
        u: f64,
        v: f64,
    },
}

/// A peer's transport, at a moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayState {
    pub playing: bool,
    /// Seconds into the song at `at_ms`.
    pub position: f64,
    /// When `position` was true, in the shared clock's milliseconds.
    pub at_ms: f64,
    pub rate: f64,
}

impl PlayState {
    /// Where the peer's playhead is at `now_ms` (shared clock).
    #[must_use]
    pub fn position_at(&self, now_ms: f64) -> f64 {
        if self.playing {
            ((now_ms - self.at_ms) / 1000.0).mul_add(self.rate, self.position)
        } else {
            self.position
        }
    }
}

// ── codecs ──────────────────────────────────────────────────────────────

fn map(entries: Vec<(&str, LoroValue)>) -> LoroValue {
    let m: std::collections::HashMap<String, LoroValue> = entries
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    LoroValue::from(m)
}

fn opt_f64(v: Option<f64>) -> LoroValue {
    v.map_or(LoroValue::Null, LoroValue::Double)
}

fn strings(v: &[String]) -> LoroValue {
    LoroValue::from(
        v.iter()
            .map(|s| LoroValue::from(s.as_str()))
            .collect::<Vec<_>>(),
    )
}

struct Fields(std::collections::HashMap<String, LoroValue>);

impl Fields {
    fn of(v: &LoroValue) -> Option<Self> {
        match v {
            LoroValue::Map(m) => Some(Self(
                m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            )),
            _ => None,
        }
    }
    fn f64(&self, k: &str) -> Option<f64> {
        match self.0.get(k)? {
            LoroValue::Double(d) => Some(*d),
            LoroValue::I64(i) => i32::try_from(*i).ok().map(f64::from),
            _ => None,
        }
    }
    fn bool(&self, k: &str) -> Option<bool> {
        match self.0.get(k)? {
            LoroValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
    fn string(&self, k: &str) -> Option<String> {
        match self.0.get(k)? {
            LoroValue::String(s) => Some(s.to_string()),
            _ => None,
        }
    }
    fn binary(&self, k: &str) -> Option<Vec<u8>> {
        match self.0.get(k)? {
            LoroValue::Binary(b) => Some(b.to_vec()),
            _ => None,
        }
    }
    fn strings(&self, k: &str) -> Vec<String> {
        match self.0.get(k) {
            Some(LoroValue::List(l)) => l
                .iter()
                .filter_map(|v| match v {
                    LoroValue::String(s) => Some(s.to_string()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

impl PeerState {
    #[must_use]
    pub fn encode(&self) -> LoroValue {
        map(vec![
            ("name", LoroValue::from(self.name.as_str())),
            ("color", LoroValue::I64(i64::from(self.color))),
            (
                "song",
                self.song
                    .as_deref()
                    .map_or(LoroValue::Null, LoroValue::from),
            ),
            ("view", LoroValue::from(self.view.as_str())),
            ("edit_cursor", opt_f64(self.edit_cursor)),
            ("sel_start", opt_f64(self.time_selection.map(|s| s.0))),
            ("sel_end", opt_f64(self.time_selection.map(|s| s.1))),
            ("tracks", strings(&self.selected_tracks)),
            ("items", strings(&self.selected_items)),
            (
                "caret_anchor",
                self.chart_caret
                    .as_ref()
                    .map_or(LoroValue::Null, |c| LoroValue::from(c.0.clone())),
            ),
            (
                "caret_head",
                self.chart_caret
                    .as_ref()
                    .map_or(LoroValue::Null, |c| LoroValue::from(c.1.clone())),
            ),
        ])
    }

    #[must_use]
    pub fn decode(v: &LoroValue) -> Option<Self> {
        let f = Fields::of(v)?;
        let color = match f.0.get("color") {
            Some(LoroValue::I64(c)) => u32::try_from(*c).unwrap_or(0),
            _ => 0,
        };
        Some(Self {
            name: f.string("name").unwrap_or_default(),
            color,
            song: f.string("song"),
            view: f.string("view").unwrap_or_default(),
            edit_cursor: f.f64("edit_cursor"),
            time_selection: f.f64("sel_start").zip(f.f64("sel_end")),
            selected_tracks: f.strings("tracks"),
            selected_items: f.strings("items"),
            chart_caret: f.binary("caret_anchor").zip(f.binary("caret_head")),
        })
    }
}

impl Pointer {
    /// Encoded with the sender's clock, `t_ms`, for interpolation.
    #[must_use]
    pub fn encode(&self, t_ms: f64) -> LoroValue {
        match self {
            Self::Timeline { at, track, y } => map(vec![
                ("kind", LoroValue::from("timeline")),
                ("t", LoroValue::Double(t_ms)),
                ("at", LoroValue::Double(*at)),
                ("y", LoroValue::Double(*y)),
                (
                    "track",
                    track.as_deref().map_or(LoroValue::Null, LoroValue::from),
                ),
            ]),
            Self::Chart { x, y } => map(vec![
                ("kind", LoroValue::from("chart")),
                ("t", LoroValue::Double(t_ms)),
                ("x", LoroValue::Double(*x)),
                ("y", LoroValue::Double(*y)),
            ]),
            Self::Region { region, x, y } => map(vec![
                ("kind", LoroValue::from("region")),
                ("t", LoroValue::Double(t_ms)),
                ("region", LoroValue::from(region.as_str())),
                ("x", LoroValue::Double(*x)),
                ("y", LoroValue::Double(*y)),
            ]),
            Self::Anchor { panel, key, u, v } => map(vec![
                ("kind", LoroValue::from("anchor")),
                ("t", LoroValue::Double(t_ms)),
                ("panel", LoroValue::from(panel.as_str())),
                ("key", LoroValue::from(key.as_str())),
                ("u", LoroValue::Double(*u)),
                ("v", LoroValue::Double(*v)),
            ]),
        }
    }

    /// The pointer and the sender's time.
    #[must_use]
    pub fn decode(v: &LoroValue) -> Option<(Self, f64)> {
        let f = Fields::of(v)?;
        let t = f.f64("t")?;
        let pointer = match f.string("kind")?.as_str() {
            "timeline" => Self::Timeline {
                at: f.f64("at")?,
                track: f.string("track"),
                y: f.f64("y").unwrap_or(0.5),
            },
            "chart" => Self::Chart {
                x: f.f64("x")?,
                y: f.f64("y")?,
            },
            "region" => Self::Region {
                region: f.string("region")?,
                x: f.f64("x")?,
                y: f.f64("y")?,
            },
            "anchor" => Self::Anchor {
                panel: f.string("panel")?,
                key: f.string("key")?,
                u: f.f64("u")?,
                v: f.f64("v")?,
            },
            _ => return None,
        };
        Some((pointer, t))
    }
}

impl PlayState {
    #[must_use]
    pub fn encode(&self) -> LoroValue {
        map(vec![
            ("playing", LoroValue::Bool(self.playing)),
            ("position", LoroValue::Double(self.position)),
            ("at", LoroValue::Double(self.at_ms)),
            ("rate", LoroValue::Double(self.rate)),
        ])
    }

    #[must_use]
    pub fn decode(v: &LoroValue) -> Option<Self> {
        let f = Fields::of(v)?;
        Some(Self {
            playing: f.bool("playing")?,
            position: f.f64("position")?,
            at_ms: f.f64("at")?,
            rate: f.f64("rate").unwrap_or(1.0),
        })
    }
}

// ── sending: throttle ───────────────────────────────────────────────────

/// Rate-limits a stream of values to at most one per `interval_ms`.
///
/// Never drops the last one: a pointer that stops moving must end where
/// it stopped, not at the last sample the throttle happened to let through.
#[derive(Debug, Clone)]
pub struct Throttle<T> {
    interval_ms: f64,
    last_sent_ms: Option<f64>,
    pending: Option<T>,
}

impl<T: PartialEq + Clone> Throttle<T> {
    #[must_use]
    pub const fn new(interval_ms: f64) -> Self {
        Self {
            interval_ms,
            last_sent_ms: None,
            pending: None,
        }
    }

    /// Offer a new value at `now_ms`. Returns it if it should go out now;
    /// otherwise it is held for [`Self::flush`].
    pub fn offer(&mut self, value: T, now_ms: f64) -> Option<T> {
        match self.last_sent_ms {
            Some(last) if now_ms - last < self.interval_ms => {
                self.pending = Some(value);
                None
            }
            _ => {
                self.last_sent_ms = Some(now_ms);
                self.pending = None;
                Some(value)
            }
        }
    }

    /// The held value, once its turn has come. Call on a timer.
    pub fn flush(&mut self, now_ms: f64) -> Option<T> {
        let due = self
            .last_sent_ms
            .is_none_or(|last| now_ms - last >= self.interval_ms);
        if due && let Some(value) = self.pending.take() {
            self.last_sent_ms = Some(now_ms);
            return Some(value);
        }
        None
    }
}

// ── receiving: interpolation ────────────────────────────────────────────

/// How far behind real time remote pointers are drawn, so there is almost
/// always a sample on each side of the moment being drawn.
pub const INTERPOLATION_DELAY_MS: f64 = 100.0;

/// A remote pointer's recent samples, for smooth drawing.
///
/// Times are the SENDER's clock. The trail anchors that clock to the
/// receiver's on arrival, so neither needs to agree with the other.
#[derive(Debug, Clone, Default)]
pub struct PointerTrail {
    samples: VecDeque<(f64, Pointer)>,
    /// Receiver time minus sender time, from the newest sample.
    offset_ms: f64,
}

impl PointerTrail {
    const KEEP: usize = 16;

    /// A sample arrived at `local_ms` (receiver clock).
    pub fn push(&mut self, pointer: Pointer, sent_ms: f64, local_ms: f64) {
        if self.samples.back().is_some_and(|(t, _)| sent_ms <= *t) {
            return; // out of order or repeated
        }
        self.offset_ms = local_ms - sent_ms;
        self.samples.push_back((sent_ms, pointer));
        while self.samples.len() > Self::KEEP {
            self.samples.pop_front();
        }
    }

    /// Where to draw the pointer at `local_ms`, in THIS window's pixels:
    /// each of the two samples around the moment is placed with `place`,
    /// then the pixels are interpolated — so a pointer moving from one
    /// track to the next, or from the chart to the mixer, glides across
    /// the screen instead of jumping. `place` answers `None` for a place
    /// this window does not show; then the other sample is used alone.
    #[must_use]
    pub fn screen_at(
        &self,
        local_ms: f64,
        place: impl Fn(&Pointer) -> Option<(f64, f64)>,
    ) -> Option<(f64, f64)> {
        let t = local_ms - self.offset_ms - INTERPOLATION_DELAY_MS;
        let mut before: Option<&(f64, Pointer)> = None;
        for sample in &self.samples {
            if sample.0 <= t {
                before = Some(sample);
                continue;
            }
            let Some(prev) = before else {
                return place(&sample.1);
            };
            let k = (t - prev.0) / (sample.0 - prev.0);
            // Into a place this view cannot show: gone at once, not left
            // where it last was.
            return match (place(&prev.1), place(&sample.1)) {
                (Some(a), Some(b)) => {
                    Some(((b.0 - a.0).mul_add(k, a.0), (b.1 - a.1).mul_add(k, a.1)))
                }
                (_, b) => b,
            };
        }
        before.and_then(|s| place(&s.1))
    }

    /// The latest pointer (what kind of place it is in now).
    #[must_use]
    pub fn latest(&self) -> Option<&Pointer> {
        self.samples.back().map(|s| &s.1)
    }

    /// Where to draw the pointer at `local_ms`.
    #[must_use]
    pub fn at(&self, local_ms: f64) -> Option<Pointer> {
        let t = local_ms - self.offset_ms - INTERPOLATION_DELAY_MS;
        let mut before: Option<&(f64, Pointer)> = None;
        for sample in &self.samples {
            if sample.0 <= t {
                before = Some(sample);
            } else {
                let Some(prev) = before else {
                    return Some(sample.1.clone());
                };
                return Some(lerp(&prev.1, &sample.1, (t - prev.0) / (sample.0 - prev.0)));
            }
        }
        before.map(|s| s.1.clone())
    }
}

fn lerp(a: &Pointer, b: &Pointer, k: f64) -> Pointer {
    let mix = |x: f64, y: f64| (y - x).mul_add(k, x);
    match (a, b) {
        // Within one track, both ways; across tracks the screen-space
        // path ([`PointerTrail::screen_at`]) is what glides.
        (
            Pointer::Timeline {
                at: a_at,
                track: ta,
                y: ay,
            },
            Pointer::Timeline {
                at: b_at,
                track,
                y: by,
            },
        ) => Pointer::Timeline {
            at: mix(*a_at, *b_at),
            track: track.clone(),
            y: if ta == track { mix(*ay, *by) } else { *by },
        },
        (Pointer::Chart { x: ax, y: ay }, Pointer::Chart { x: bx, y: by }) => Pointer::Chart {
            x: mix(*ax, *bx),
            y: mix(*ay, *by),
        },
        (
            Pointer::Anchor {
                panel: pa,
                key: ka,
                u: ua,
                v: va,
            },
            Pointer::Anchor {
                panel: pb,
                key: kb,
                u: ub,
                v: vb,
            },
        ) if pa == pb && ka == kb => Pointer::Anchor {
            panel: pb.clone(),
            key: kb.clone(),
            u: mix(*ua, *ub),
            v: mix(*va, *vb),
        },
        // Across panels it jumps: halfway between two is on neither.
        (
            Pointer::Region {
                region: ra,
                x: ax,
                y: ay,
            },
            Pointer::Region {
                region: rb,
                x: bx,
                y: by,
            },
        ) if ra == rb => Pointer::Region {
            region: rb.clone(),
            x: mix(*ax, *bx),
            y: mix(*ay, *by),
        },
        _ => b.clone(),
    }
}

// ── everyone ────────────────────────────────────────────────────────────

/// One remote peer as the UI draws them.
#[derive(Debug, Clone, Default)]
pub struct Peer {
    pub state: Option<PeerState>,
    pub trail: PointerTrail,
    pub play: Option<PlayState>,
}

/// Everyone else, keyed by peer id.
#[derive(Debug, Clone, Default)]
pub struct Roster {
    pub peers: BTreeMap<String, Peer>,
}

impl Roster {
    /// Apply one presence entry that arrived at `local_ms`. `me` is this
    /// peer's id, whose own entries are ignored. `None` removes the part
    /// (the peer left, or timed out).
    pub fn apply(&mut self, me: &str, key: &str, value: Option<&LoroValue>, local_ms: f64) {
        let Some((peer, part)) = split_key(key) else {
            return;
        };
        // Only a peer's own three parts make a peer: session-wide entries
        // (the shared transport, `session/transport`) are not somebody.
        if peer == me || ![STATE, POINTER, PLAY].contains(&part) {
            return;
        }
        let Some(value) = value else {
            if part == STATE {
                self.peers.remove(peer);
            } else if let Some(p) = self.peers.get_mut(peer) {
                match part {
                    POINTER => p.trail = PointerTrail::default(),
                    PLAY => p.play = None,
                    _ => {}
                }
            }
            return;
        };
        let entry = self.peers.entry(peer.to_string()).or_default();
        match part {
            STATE => entry.state = PeerState::decode(value),
            POINTER => {
                if let Some((pointer, sent)) = Pointer::decode(value) {
                    entry.trail.push(pointer, sent, local_ms);
                }
            }
            PLAY => entry.play = PlayState::decode(value),
            _ => {}
        }
    }
}

/// A peer's colour from their id: a stable hue, bright enough to read on
/// the dark arrangement.
#[must_use]
pub fn color_for(peer: &str) -> u32 {
    let hash = peer.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let hue = f64::from(hash % 360);
    hsl_to_rgb(hue, 0.7, 0.62)
}

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> u32 {
    let chroma = (1.0 - 2.0f64.mul_add(lightness, -1.0).abs()) * saturation;
    let sector = hue / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match sector {
        s if s < 1.0 => (chroma, second, 0.0),
        s if s < 2.0 => (second, chroma, 0.0),
        s if s < 3.0 => (0.0, chroma, second),
        s if s < 4.0 => (0.0, second, chroma),
        s if s < 5.0 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let base = lightness - chroma / 2.0;
    let byte = |channel: f64| {
        let scaled = ((channel + base) * 255.0).round().clamp(0.0, 255.0);
        // In range by the clamp above.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions
        )]
        let value = scaled as u32;
        value
    };
    (byte(red) << 16) | (byte(green) << 8) | byte(blue)
}
