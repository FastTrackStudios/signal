//! Automation from the Action List: lanes shown, read and written, and a
//! parameter automated **by section**.
//!
//! By section is how a mix is finished: the kick's top end up in the loud
//! choruses and the instrumentals, a vocal's reverb up in a sparse verse.
//!
//! An envelope is named the way a person names it: `Volume`, `Pan`,
//! `Width`, `Mute` (`Volume pre-FX`, `Pan pre-FX`), or a plugin's parameter
//! as `FX/parameter` (`FTS EQ/SSL HF Gain`; the FX and the parameter
//! as `FX_SET` names them — any part of the FX's name or its `#place`, a
//! repeated parameter with its count, `Threshold 2`).
//!
//! Values are given in the envelope's own terms: dB for volume, −100 (left)
//! to 100 (right) for pan, 0/1 for mute, percent for width — and for a
//! plugin's parameter either its text in `FX_SET`'s words (`"+3 dB"`,
//! `"=0.4"`) or a number from 0 to 1.
//!
//! A section is picked by its name (`CH 2`), its kind (`CH`: every
//! chorus), or what is in it (see [`Analysis`]): `loud`, `quiet`, `vocal`,
//! `instrumental` — and those together (`loud instrumental`).

use std::collections::HashMap;

use facet::Facet;

use daw::service::{ProjectContext, TrackRef, Tracks};
use daw_proto::automation::{
    AddAutomationItemParams, Automation, EnvelopeLocation, EnvelopePoint, EnvelopeRef,
    EnvelopeShape, EnvelopeType, SetAutomationItemPointsParams,
};
use daw_proto::primitives::{Duration, PositionInSeconds};

use crate::automation::Kind;

type Daw = daw::standalone::Standalone;

/// An envelope found by its name on a track.
#[derive(Clone, Debug)]
pub(crate) struct Found {
    pub target: EnvelopeRef,
    pub kind: Kind,
    /// The plugin's name and the parameter, for a parameter.
    pub param: Option<(String, daw_proto::fx::FxParameter)>,
    /// What it is called in a lane.
    pub name: String,
}

impl Found {
    /// `value` said in this envelope's own terms.
    pub(crate) fn say(&self, value: f64) -> String {
        if let Some(choice) = crate::automation::choice_at(self.choices(), value) {
            return choice.to_owned();
        }
        crate::automation::say(
            self.kind,
            self.param.as_ref().map(|(fx, p)| (fx.as_str(), p.index)),
            value,
        )
    }
}

impl Found {
    /// A stepped parameter's choices (a scene, a mode); empty otherwise.
    pub(crate) fn choices(&self) -> &[(f64, String)] {
        self.param
            .as_ref()
            .map_or(&[], |(_, p)| p.step_labels.as_slice())
    }

    /// Whether it steps — a switch, not a level: it jumps at a section's
    /// downbeat rather than ramping through the choices between.
    pub(crate) fn stepped(&self) -> bool {
        !self.choices().is_empty() || self.kind == Kind::Mute
    }
}

/// The envelope `name` names on `track`.
pub(crate) fn find(
    daw: &Daw,
    project: &ProjectContext,
    track: &str,
    name: &str,
) -> Result<Found, String> {
    let wanted = name.trim().to_ascii_lowercase();
    let track_env = |ty: EnvelopeType, label: &str| Found {
        target: EnvelopeRef::Type(ty),
        kind: Kind::of(ty),
        param: None,
        name: label.to_owned(),
    };
    match wanted.as_str() {
        "volume" | "vol" => return Ok(track_env(EnvelopeType::Volume, "Volume")),
        "volume pre-fx" | "volume prefx" => {
            return Ok(track_env(EnvelopeType::VolumePrefx, "Volume (Pre-FX)"));
        }
        "pan" => return Ok(track_env(EnvelopeType::Pan, "Pan")),
        "pan pre-fx" | "pan prefx" => return Ok(track_env(EnvelopeType::PanPrefx, "Pan (Pre-FX)")),
        "width" => return Ok(track_env(EnvelopeType::Width, "Width")),
        "mute" => return Ok(track_env(EnvelopeType::Mute, "Mute")),
        _ => {}
    }
    let (fx_name, param_name) = name
        .split_once('/')
        .ok_or_else(|| format!("{name}: not Volume, Pan, Width or Mute, nor FX/parameter"))?;
    let (at, fx) = crate::fx_actions::target(daw, project, track, fx_name)?;
    let params = daw_proto::fx::Effects::parameters(daw, project.clone(), at);
    let param = crate::fx_actions::param_of(&params, param_name)?;
    Ok(Found {
        target: EnvelopeRef::FxParam {
            fx_guid: fx.guid.clone(),
            param_index: param.index,
        },
        kind: Kind::Param,
        param: Some((fx.name.clone(), param.clone())),
        name: format!("{}/{}", fx.name, param.name),
    })
}

/// A value given in an envelope's own terms.
#[derive(Facet, Clone, Debug, Default)]
pub struct Value {
    /// A number: dB (volume), −100…100 (pan), 0/1 (mute), percent
    /// (width), 0…1 (a parameter).
    pub value: Option<f64>,
    /// A parameter's value in `FX_SET`'s words: as its plugin writes it
    /// (`"+3 dB"`, `"8 kHz"`), a choice's label, or `=0.42`.
    pub text: Option<String>,
}

/// `v` as the engine's value for envelope `found`.
pub(crate) fn engine_value(found: &Found, v: &Value) -> Result<f64, String> {
    if let Some(text) = &v.text {
        let (fx, param) = found.param.as_ref().ok_or_else(|| {
            format!(
                "{text}: text is a plugin parameter's; give {} a number",
                found.name
            )
        })?;
        return crate::fx_actions::value_of(fx, param, text);
    }
    let n = v
        .value
        .ok_or_else(|| format!("{}: a value or a text", found.name))?;
    Ok(match found.kind {
        Kind::Volume => 10_f64.powf(n / 20.0),
        Kind::Pan => f64::midpoint(n.clamp(-100.0, 100.0) / 100.0, 1.0),
        Kind::Mute => {
            if n > 0.5 {
                1.0
            } else {
                0.0
            }
        }
        Kind::Width => (n / 100.0).clamp(0.0, 1.0),
        Kind::Param => n.clamp(0.0, 1.0),
        Kind::Leveler(_) => n,
    })
}

/// The track a name resolves to.
fn track_of(daw: &Daw, project: &ProjectContext, what: &str) -> Result<daw_proto::Track, String> {
    let guid = crate::actions::resolve(daw, project, what)?;
    Tracks::get(daw, project.clone(), TrackRef::Guid(guid))
        .ok_or_else(|| format!("{what}: no such track"))
}

fn location(track: &str, found: &Found) -> EnvelopeLocation {
    EnvelopeLocation::new(TrackRef::Guid(track.to_owned()), found.target.clone())
}

/// Shown in a lane (made if it was not there): visible, in a lane of its
/// own at `height`.
pub(crate) fn show(daw: &Daw, project: &ProjectContext, track: &str, found: &Found, height: u32) {
    let at = location(track, found);
    Automation::set_visible(daw, project.clone(), at.clone(), true);
    let _ = Automation::set_lane(
        daw,
        project.clone(),
        at,
        daw_proto::automation::LaneParams::own(height),
    );
}

/// Replace every point of the envelope from `from` to `to` (seconds) with
/// `points`.
fn put(
    daw: &Daw,
    project: &ProjectContext,
    at: &EnvelopeLocation,
    span: (f64, f64),
    points: &[EnvelopePoint],
) {
    Automation::delete_points_in_range(
        daw,
        project.clone(),
        at.clone(),
        daw_proto::automation::TimeRangeParams::new(
            PositionInSeconds::from_seconds(span.0),
            PositionInSeconds::from_seconds(span.1),
        ),
    );
    for p in points {
        let _ = Automation::add_point(
            daw,
            project.clone(),
            at.clone(),
            daw_proto::automation::AddPointParams::new(p.time, p.value, p.shape),
        );
    }
}

fn point(t: f64, value: f64, shape: EnvelopeShape) -> EnvelopePoint {
    EnvelopePoint {
        time: PositionInSeconds::from_seconds(t.max(0.0)),
        value,
        shape,
        ..EnvelopePoint::default()
    }
}

// ── What is in a section ───────────────────────────────────────────────

/// A section and what is in it.
#[derive(Facet, Clone, Debug)]
pub struct Analysis {
    pub name: String,
    pub start: f64,
    pub end: f64,
    /// How loud everything playing is together there, dB (the tracks'
    /// recordings at their faders — a picture of the arrangement, not a
    /// meter's reading).
    pub level_db: f64,
    /// How much of it has someone singing (0–1).
    pub sung: f64,
    /// Louder than halfway between the song's quietest and loudest
    /// sections.
    pub loud: bool,
    /// Someone sings through a quarter of it or more.
    pub vocal: bool,
}

/// A section's kind: its name up to its number (`CH 2` → `CH`, `VS 1B`
/// → `VS`).
fn kind_of(name: &str) -> String {
    name.split(|c: char| c.is_ascii_digit())
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_uppercase()
}

/// How loud each section is with everything playing, and how much of it is
/// sung — read from the recordings' peaks.
pub fn analyse(daw: &Daw, project: &ProjectContext) -> Vec<Analysis> {
    use daw_proto::{ItemRef, Peaks as _, TakeRef};
    const BLOCK: u32 = 4_800;
    let sections = crate::actions::sections(daw, project);
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    // Power per section and sung time per section, summed over tracks.
    let mut power = vec![0.0_f64; sections.len()];
    let mut sung = vec![0.0_f64; sections.len()];
    for track in tracks.iter().filter(|t| !t.is_folder && !t.muted) {
        let path = crate::gain_stage::path_of(track, &by_guid);
        let vocal = path
            .first()
            .is_some_and(|top| top.trim().eq_ignore_ascii_case("vocals"));
        // A drum's trigger keys its gate and is never heard.
        if path.iter().any(|p| p.trim().eq_ignore_ascii_case("trig")) {
            continue;
        }
        for item in crate::vocal_level::playing(daw, project, track) {
            let peaks = daw.take_peaks(
                project.clone(),
                ItemRef::Guid(item.guid.clone()),
                TakeRef::Active,
                BLOCK,
            );
            let channels = usize::try_from(peaks.num_channels.max(1)).unwrap_or(1);
            let block_s = f64::from(peaks.samples_per_peak.max(1)) / peaks.sample_rate.max(1.0);
            let gain = item.volume * track.volume;
            let levels: Vec<f64> = peaks
                .peaks
                .chunks(channels.saturating_mul(2))
                .map(|c| c.iter().fold(0.0_f64, |m, v| m.max(v.abs())) * gain)
                .collect();
            let loudest = levels.iter().copied().fold(0.0_f64, f64::max);
            let start = item.position.as_seconds();
            for (k, level) in levels.iter().enumerate() {
                let t =
                    (f64::from(u32::try_from(k).unwrap_or(u32::MAX)) + 0.5).mul_add(block_s, start);
                if t > start + item.length.as_seconds() {
                    break;
                }
                let Some(s) = sections.iter().position(|s| t >= s.start && t < s.end) else {
                    continue;
                };
                if let Some(p) = power.get_mut(s) {
                    *p += level * level * block_s;
                }
                if vocal && *level > loudest * 0.063 {
                    // Within 24 dB of the take's loudest: a line, not
                    // the room between lines.
                    if let Some(v) = sung.get_mut(s) {
                        *v += block_s;
                    }
                }
            }
        }
    }
    let levels: Vec<f64> = sections
        .iter()
        .zip(&power)
        .map(|(s, p)| 10.0 * (p / (s.end - s.start).max(1e-3)).max(1e-12).log10())
        .collect();
    let lo = levels.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = levels.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let middle = f64::midpoint(lo, hi);
    sections
        .iter()
        .zip(levels)
        .zip(sung)
        .map(|((s, level_db), sung)| {
            let share = (sung / (s.end - s.start).max(1e-3)).clamp(0.0, 1.0);
            Analysis {
                name: s.name.clone(),
                start: s.start,
                end: s.end,
                level_db,
                sung: share,
                loud: level_db >= middle,
                vocal: share >= 0.25,
            }
        })
        .collect()
}

/// Whether section `a` is one `pick` names: its name, its kind, or every
/// word of `loud` / `quiet` / `vocal` / `instrumental` true of it.
fn picks(pick: &str, a: &Analysis) -> bool {
    let pick = pick.trim();
    if pick.eq_ignore_ascii_case(a.name.trim()) || pick.eq_ignore_ascii_case(&kind_of(&a.name)) {
        return true;
    }
    let words: Vec<String> = pick
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    !words.is_empty()
        && words.iter().all(|w| match w.as_str() {
            "loud" => a.loud,
            "quiet" => !a.loud,
            "vocal" | "sung" => a.vocal,
            "instrumental" | "inst" => !a.vocal,
            _ => false,
        })
}

// ── The actions ────────────────────────────────────────────────────────

static LANES: architect::action::ActionMeta = crate::actions::meta(
    "AUTOMATION_LANES",
    "automation_lanes",
    "Automation lanes",
    "A track's automation: every envelope it has (shown in a lane or not) with its mode, its points (seconds and the value in its own terms) and its automation items — and what can be automated on it: Volume, Pan, Width, Mute, and each plugin's parameters as `FX/parameter`.",
    "Automation",
    "",
);

static SHOW: architect::action::ActionMeta = crate::actions::meta(
    "AUTOMATION_SHOW",
    "automation_show",
    "Show an envelope",
    "Show `envelope` of `track` in a lane of its own under it (made if it is not there yet), or hide it (`shown`: false). `envelope` is Volume, Pan, Width, Mute, `FX/parameter` (`FTS EQ/SSL HF Gain`), `Item volume`: each item's volume curve (a leveled drum's strokes) drawn on it, its points dragged there, or `Vocal leveler`: a leveled vocal's four stages — De-Ess, De-Breath, Ride (whose points are dragged), Gate — a lane each, with each stage's switch and dial in its lane's header (their sum is drawn on each item whether they are shown or not; VOCAL_LEVELER_SET turns the dials). `height` in pixels (64).",
    "Automation",
    "",
);

static WRITE: architect::action::ActionMeta = crate::actions::meta(
    "AUTOMATION_WRITE",
    "automation_write",
    "Write automation",
    "Write points into `envelope` of `track` (Volume, Pan, Width, Mute or `FX/parameter`), replacing what was between the first and the last of them: each `at` a bar (`33`, `33:2.5`), a time (`12.5s`) or a section (`CH 1`, `CH 1 end`), with `value` in the envelope's terms (dB for volume, −100…100 for pan, 0/1 mute, percent width, 0…1 a parameter) or `text` for a plugin parameter in FX_SET's words (`\"+3 dB\"`, `\"=0.4\"`), and a `shape` (linear, step, smooth, fast start, fast end). Shows the lane. One undo step.",
    "Automation",
    "",
);

static CLEAR: architect::action::ActionMeta = crate::actions::meta(
    "AUTOMATION_CLEAR",
    "automation_clear",
    "Clear automation",
    "Delete `envelope`'s points on `track` between `from` and `to` seconds (all of them when left out), and its automation items there. One undo step.",
    "Automation",
    "",
);

static MODE: architect::action::ActionMeta = crate::actions::meta(
    "AUTOMATION_MODE",
    "automation_mode",
    "Automation mode",
    "Set `envelope`'s automation mode on `track`: trim (Trim/Read: plays, the fader trims it), read, touch, latch, write, or off (ignored).",
    "Automation",
    "",
);

static ITEM: architect::action::ActionMeta = crate::actions::meta(
    "AUTOMATION_ITEM",
    "automation_item",
    "Make an automation item",
    "Make an automation item on `envelope` of `track` from `from` to `to` (bars `33`, `33:2.5`, times `12.5s` or sections `CH 1`, `CH 1 end`): a fresh one, its curve what the envelope plays there now — or, with `pool_of` (`track`, `envelope`, `at`: a time inside one of its items), another instance of that item's pool, on this track or any other, so editing one edits all (`to` left out: the source item's length). `delete`: true deletes the item at `from` instead. Shows the lane. One undo step.",
    "Automation",
    "",
);

static SECTIONS: architect::action::ActionMeta = crate::actions::meta(
    "AUTOMATION_SECTIONS",
    "automation_sections",
    "What is in each section",
    "The song's sections and what is in each: how loud everything playing is together (`level_db`, from the recordings at their faders), how much of it is sung, and so whether it is `loud` (above halfway between the quietest and loudest sections) and `vocal` (sung through a quarter of it or more). What AUTOMATION_BY_SECTION picks sections by.",
    "Automation",
    "",
);

static BY_SECTION: architect::action::ActionMeta = crate::actions::meta(
    "AUTOMATION_BY_SECTION",
    "automation_by_section",
    "Automate by section",
    "Set `envelope` of `track` section by section — the mix move made for each part of the song (the kick's top end up in the loud choruses and the instrumentals, a vocal send up in the verses). `values`: each with a `section` that picks sections — a name (`CH 2`), a kind (`CH`: every chorus) or what is in them (`loud`, `quiet`, `vocal`, `instrumental`, and words together: `loud instrumental`) — and its `value`/`text` as AUTOMATION_WRITE takes them; the first that picks a section wins. `rest`: the value for the sections none picks (what the envelope is at now when left out). Each change moves over `ramp_beats` (1) before its section starts. `pooled`: each picked kind of section as one pooled automation item, so a chorus changed in one place changes in all (the changes then land on the downbeat). Says which value each section got. `dry_run` only says. One undo step.",
    "Automation",
    "",
);

#[derive(Facet)]
struct TrackArg {
    track: String,
}

#[derive(Facet)]
struct ShowArg {
    track: String,
    envelope: String,
    shown: Option<bool>,
    height: Option<u32>,
}

#[derive(Facet)]
struct PointArg {
    /// A bar (`33`, `33:2.5`), a time (`12.5s`) or a section (`CH 1`,
    /// `CH 1 end`).
    at: String,
    value: Option<f64>,
    text: Option<String>,
    shape: Option<String>,
}

#[derive(Facet)]
struct WriteArg {
    track: String,
    envelope: String,
    points: Vec<PointArg>,
}

#[derive(Facet)]
struct ClearArg {
    track: String,
    envelope: String,
    from: Option<f64>,
    to: Option<f64>,
}

#[derive(Facet)]
struct PoolOf {
    track: String,
    envelope: String,
    at: String,
}

#[derive(Facet)]
struct ItemArg {
    track: String,
    envelope: String,
    from: String,
    to: Option<String>,
    pool_of: Option<PoolOf>,
    delete: Option<bool>,
}

#[derive(Facet)]
struct ModeArg {
    track: String,
    envelope: String,
    mode: String,
}

#[derive(Facet)]
struct SectionValue {
    section: String,
    value: Option<f64>,
    text: Option<String>,
}

#[derive(Facet)]
struct BySectionArg {
    track: String,
    envelope: String,
    values: Vec<SectionValue>,
    rest: Option<Value>,
    ramp_beats: Option<f64>,
    pooled: Option<bool>,
    dry_run: Option<bool>,
}

/// One point as said back.
#[derive(Facet)]
struct PointOut {
    time: f64,
    value: String,
    shape: String,
}

#[derive(Facet)]
struct ItemOut {
    pool: i32,
    position: f64,
    length: f64,
    points: Vec<PointOut>,
}

#[derive(Facet)]
struct EnvelopeOut {
    envelope: String,
    shown: bool,
    mode: String,
    points: Vec<PointOut>,
    items: Vec<ItemOut>,
}

#[derive(Facet)]
struct LanesOut {
    track: String,
    envelopes: Vec<EnvelopeOut>,
    can_automate: Vec<String>,
}

/// One section as a move by section set it.
#[derive(Facet, Clone, Debug)]
pub struct SectionSet {
    pub section: String,
    pub start: f64,
    pub value: String,
    pub picked_by: String,
}

fn said(kind: Kind, param: Option<(&str, u32)>, p: &EnvelopePoint) -> PointOut {
    PointOut {
        time: p.time.as_seconds(),
        value: crate::automation::say(kind, param, p.value),
        shape: format!("{:?}", p.shape),
    }
}

/// Done in one undo step, then the window told to read the session again.
fn undoable<T>(daw: &Daw, project: &ProjectContext, label: &str, f: impl FnOnce() -> T) -> T {
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let out = f();
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    crate::studio::request_resync();
    out
}

fn lanes(a: &TrackArg) -> Result<LanesOut, String> {
    let (daw, project) = crate::actions::song()?;
    let track = track_of(daw, &project, &a.track)?;
    let shown = Automation::envelopes(daw, project.clone(), TrackRef::Guid(track.guid.clone()));
    let chain = daw_proto::fx::FxChainContext::Track(track.guid.clone());
    let fxs = daw_proto::fx::Effects::list(daw, project.clone(), chain.clone());
    let mut envelopes = Vec::new();
    for e in shown.iter().filter(|e| !e.name.starts_with("Send ")) {
        let fx_name = e
            .fx_guid
            .as_ref()
            .and_then(|g| fxs.iter().find(|f| f.guid == *g))
            .map(|f| f.name.clone());
        let param = fx_name.as_deref().zip(e.param_index);
        let target = match (&e.fx_guid, e.param_index) {
            (Some(fx_guid), Some(param_index)) => EnvelopeRef::FxParam {
                fx_guid: fx_guid.clone(),
                param_index,
            },
            _ => EnvelopeRef::Type(e.envelope_type),
        };
        let at = EnvelopeLocation::new(TrackRef::Guid(track.guid.clone()), target);
        let kind = Kind::of(e.envelope_type);
        let points = Automation::points(daw, project.clone(), at.clone());
        let items = Automation::automation_items(daw, project.clone(), at.clone())
            .into_iter()
            .map(|item| ItemOut {
                pool: item.pool_id,
                position: item.position.as_seconds(),
                length: item.length.as_seconds(),
                points: Automation::automation_item_points(
                    daw,
                    project.clone(),
                    at.clone(),
                    item.index,
                )
                .iter()
                .map(|p| said(kind, param, p))
                .collect(),
            })
            .collect();
        let name = match (&fx_name, e.param_index) {
            (Some(fx), Some(index)) => daw_proto::fx::Effects::parameters(
                daw,
                project.clone(),
                daw_proto::fx::FxTarget::new(
                    chain.clone(),
                    daw_proto::fx::FxRef::Guid(e.fx_guid.clone().unwrap_or_default()),
                ),
            )
            .into_iter()
            .find(|p| p.index == index)
            .map_or_else(|| e.name.clone(), |p| format!("{fx}/{}", p.name)),
            _ => e.name.clone(),
        };
        envelopes.push(EnvelopeOut {
            envelope: name,
            shown: e.visible,
            mode: format!("{:?}", e.automation_mode),
            points: points.iter().map(|p| said(kind, param, p)).collect(),
            items,
        });
    }
    let mut can_automate: Vec<String> = ["Volume", "Pan", "Width", "Mute"]
        .map(str::to_owned)
        .to_vec();
    for (i, fx) in fxs.iter().enumerate() {
        let target = daw_proto::fx::FxTarget::new(
            chain.clone(),
            daw_proto::fx::FxRef::Guid(fx.guid.clone()),
        );
        let count = daw_proto::fx::Effects::parameters(daw, project.clone(), target).len();
        can_automate.push(format!(
            "{}/… — #{} in the chain, {count} parameters (FX_PARAMS names them)",
            fx.name,
            i.saturating_add(1)
        ));
    }
    Ok(LanesOut {
        track: track.name,
        envelopes,
        can_automate,
    })
}

fn by_section(a: &BySectionArg) -> Result<Vec<SectionSet>, String> {
    let (daw, project) = crate::actions::song()?;
    let values = a
        .values
        .iter()
        .map(|v| {
            (
                v.section.clone(),
                Value {
                    value: v.value,
                    text: v.text.clone(),
                },
            )
        })
        .collect();
    let by = BySection {
        track: a.track.clone(),
        envelope: a.envelope.clone(),
        values,
        rest: a.rest.clone(),
        ramp_beats: a.ramp_beats.unwrap_or(1.0),
        pooled: a.pooled.unwrap_or(false),
    };
    automate_by_section(daw, &project, &by, a.dry_run.unwrap_or(false))
}

/// A move made section by section ([`automate_by_section`]).
#[derive(Clone, Debug)]
pub struct BySection {
    /// The track, as `SESSION_TRACKS` names it.
    pub track: String,
    /// The envelope, as [`find`] reads names.
    pub envelope: String,
    /// What picks sections ([`picks`]) and the value they get; the first
    /// that picks one wins.
    pub values: Vec<(String, Value)>,
    /// The sections none picks; what the envelope is at now when `None`.
    pub rest: Option<Value>,
    /// How long each change takes, in beats, ending as its section starts.
    pub ramp_beats: f64,
    /// Each picked kind of section one pooled automation item.
    pub pooled: bool,
}

// r[impl automation.by-section]
/// Set an envelope section by section — what `AUTOMATION_BY_SECTION`
/// does — and say which value each section got. `dry` only says.
///
/// # Errors
///
/// No such track or envelope, a value the envelope does not read, or a
/// song with no sections.
pub fn automate_by_section(
    daw: &Daw,
    project: &ProjectContext,
    a: &BySection,
    dry: bool,
) -> Result<Vec<SectionSet>, String> {
    let project = project.clone();
    let track = track_of(daw, &project, &a.track)?;
    let found = find(daw, &project, &track.guid, &a.envelope)?;
    let at = location(&track.guid, &found);
    let sections = analyse(daw, &project);
    if sections.is_empty() {
        return Err("the song has no sections to automate by".to_owned());
    }
    let values: Vec<(String, f64)> = a
        .values
        .iter()
        .map(|(pick, v)| engine_value(&found, v).map(|x| (pick.clone(), x)))
        .collect::<Result<_, String>>()?;
    let rest = match &a.rest {
        Some(v) => engine_value(&found, v)?,
        None => current(daw, &project, &track.guid, &found, &at),
    };
    // Each section's value and what picked it.
    let plan: Vec<(&Analysis, f64, String)> = sections
        .iter()
        .map(|s| {
            values
                .iter()
                .find(|(pick, _)| picks(pick, s))
                .map_or((s, rest, String::new()), |(pick, v)| (s, *v, pick.clone()))
        })
        .collect();
    let out: Vec<SectionSet> = plan
        .iter()
        .map(|(s, v, by)| SectionSet {
            section: s.name.clone(),
            start: s.start,
            value: found.say(*v),
            picked_by: by.clone(),
        })
        .collect();
    if dry {
        return Ok(out);
    }
    // A switch (a scene, a mute) changes on the downbeat: a ramp would
    // pass through the choices between.
    let ramp_beats = if found.stepped() {
        0.0
    } else {
        a.ramp_beats.max(0.0)
    };
    let pooled = a.pooled;
    let first = sections.first().map_or(0.0, |s| s.start);
    let last = sections.last().map_or(0.0, |s| s.end);
    undoable(daw, &project, "Automate by section", || {
        show(daw, &project, &track.guid, &found, 64);
        // What was there over the song's sections goes: its points and
        // its items.
        let items = Automation::automation_items(daw, project.clone(), at.clone());
        for item in items.iter().rev() {
            let start = item.position.as_seconds();
            if start >= first - 1e-6 && start < last {
                let _ = Automation::delete_automation_item(
                    daw,
                    project.clone(),
                    at.clone(),
                    item.index,
                );
            }
        }
        if pooled {
            write_pooled(daw, &project, &at, &plan, (rest, first, last));
        } else {
            write_ramps(daw, &project, &at, &plan, (ramp_beats, first, last));
        }
    });
    Ok(out)
}

/// Each section's plan: the section, its value, and what picked it (empty
/// for none).
type Plan<'a> = [(&'a Analysis, f64, String)];

/// A plan as one pooled automation item per picked kind of section, every
/// other section at `rest` (the envelope's own points from `first` to
/// `last`).
fn write_pooled(
    daw: &Daw,
    project: &ProjectContext,
    at: &EnvelopeLocation,
    plan: &Plan,
    (rest, first, last): (f64, f64, f64),
) {
    let flat = [
        point(first, rest, EnvelopeShape::Linear),
        point(last, rest, EnvelopeShape::Linear),
    ];
    put(daw, project, at, (first - 1e-6, last + 1e-6), &flat);
    let mut pools: HashMap<String, i32> = HashMap::new();
    for (s, v, by) in plan {
        if by.is_empty() {
            continue;
        }
        let pool = pools.get(by).copied().unwrap_or(-1);
        let made = Automation::add_automation_item(
            daw,
            project.clone(),
            at.clone(),
            AddAutomationItemParams::pooled(
                pool,
                PositionInSeconds::from_seconds(s.start),
                Duration::from_seconds(s.end - s.start),
            ),
        );
        let Ok(index) = made else { continue };
        if pool >= 0 {
            continue;
        }
        // The pool's first instance: its curve, which every later one of
        // the kind shares.
        let id = Automation::automation_items(daw, project.clone(), at.clone())
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .map_or(-1, |i| i.pool_id);
        pools.insert(by.clone(), id);
        let length = s.end - s.start;
        let _ = Automation::set_automation_item_points(
            daw,
            project.clone(),
            at.clone(),
            SetAutomationItemPointsParams {
                index,
                points: vec![
                    point(0.0, *v, EnvelopeShape::Linear),
                    point(length, *v, EnvelopeShape::Linear),
                ],
            },
        );
    }
}

/// A plan as the envelope's own points: each section at its value, each
/// change an S over `ramp_beats` ending as its section starts.
fn write_ramps(
    daw: &Daw,
    project: &ProjectContext,
    at: &EnvelopeLocation,
    plan: &Plan,
    (ramp_beats, first, last): (f64, f64, f64),
) {
    let mut points = Vec::new();
    let mut was: Option<f64> = None;
    for (s, v, _) in plan {
        let beat = 60.0 / daw_proto::TempoMap::get_tempo_at(daw, project.clone(), s.start).max(1.0);
        let ramp = (ramp_beats * beat).min((s.end - s.start) / 2.0);
        match was {
            Some(before) if (before - v).abs() > 1e-9 => {
                points.push(point(s.start - ramp, before, EnvelopeShape::SlowStartEnd));
                points.push(point(s.start, *v, EnvelopeShape::Linear));
            }
            Some(_) => {}
            None => points.push(point(s.start, *v, EnvelopeShape::Linear)),
        }
        was = Some(*v);
    }
    if let Some(v) = was {
        points.push(point(last, v, EnvelopeShape::Linear));
    }
    put(daw, project, at, (first, last + 1e-6), &points);
}

/// What envelope `found` is at now, before any section is set: its value
/// at the song's start if it has points, else the parameter's own or the
/// envelope's neutral value.
fn current(
    daw: &Daw,
    project: &ProjectContext,
    track: &str,
    found: &Found,
    at: &EnvelopeLocation,
) -> f64 {
    if !Automation::points(daw, project.clone(), at.clone()).is_empty() {
        return Automation::value_at(daw, project.clone(), at.clone(), PositionInSeconds::ZERO);
    }
    match &found.target {
        EnvelopeRef::FxParam {
            fx_guid,
            param_index,
        } => {
            let chain = daw_proto::fx::FxChainContext::Track(track.to_owned());
            daw_proto::fx::Effects::parameters(
                daw,
                project.clone(),
                daw_proto::fx::FxTarget::new(chain, daw_proto::fx::FxRef::Guid(fx_guid.clone())),
            )
            .into_iter()
            .find(|p| p.index == *param_index)
            .map_or(0.5, |p| p.value)
        }
        _ => crate::automation::neutral(found.kind),
    }
}

/// The automation item of the envelope at `at` playing at `t`, if one: its
/// index, pool and length.
fn item_at(
    daw: &Daw,
    project: &ProjectContext,
    at: &EnvelopeLocation,
    t: f64,
) -> Option<(u32, i32, f64)> {
    Automation::automation_items(daw, project.clone(), at.clone())
        .into_iter()
        .rev()
        .find(|i| {
            let start = i.position.as_seconds();
            t >= start && t <= start + i.length.as_seconds()
        })
        .map(|i| (i.index, i.pool_id, i.length.as_seconds()))
}

/// What the envelope at `at` plays from `from` to `to`, as an automation
/// item's own curve (seconds into the item).
fn curve_between(
    daw: &Daw,
    project: &ProjectContext,
    at: &EnvelopeLocation,
    (from, to): (f64, f64),
    rest: f64,
) -> Vec<EnvelopePoint> {
    let points = Automation::points(daw, project.clone(), at.clone());
    let items: Vec<_> = Automation::automation_items(daw, project.clone(), at.clone())
        .into_iter()
        .map(|i| {
            let curve =
                Automation::automation_item_points(daw, project.clone(), at.clone(), i.index);
            (i, curve)
        })
        .collect();
    let played = daw_proto::automation::played(&points, &items);
    let value = |t: f64| daw_proto::automation::value_at(&played, t).unwrap_or(rest);
    let mut curve = vec![point(0.0, value(from), EnvelopeShape::Linear)];
    curve.extend(
        played
            .iter()
            .filter(|p| p.time.as_seconds() > from && p.time.as_seconds() < to)
            .map(|p| EnvelopePoint {
                time: PositionInSeconds::from_seconds(p.time.as_seconds() - from),
                ..p.clone()
            }),
    );
    curve.push(point(to - from, value(to), EnvelopeShape::Linear));
    curve
}

/// `AUTOMATION_ITEM`: an automation item made (fresh, or another instance
/// of a pool) or deleted.
fn item(a: &ItemArg) -> Result<String, String> {
    let (daw, project) = crate::actions::song()?;
    let track = track_of(daw, &project, &a.track)?;
    let found = find(daw, &project, &track.guid, &a.envelope)?;
    let at = location(&track.guid, &found);
    let from = crate::fx_actions::moment(daw, &project, &a.from)?;
    if a.delete.unwrap_or(false) {
        let (index, ..) = item_at(daw, &project, &at, from)
            .ok_or_else(|| format!("no automation item at {}", a.from))?;
        undoable(daw, &project, "Delete an automation item", || {
            let _ = Automation::delete_automation_item(daw, project.clone(), at.clone(), index);
        });
        return Ok(format!("the item at {} on {} deleted", a.from, found.name));
    }
    let pooled = match &a.pool_of {
        Some(of) => {
            let source = track_of(daw, &project, &of.track)?;
            let source_env = find(daw, &project, &source.guid, &of.envelope)?;
            let t = crate::fx_actions::moment(daw, &project, &of.at)?;
            let (_, pool, length) = item_at(daw, &project, &location(&source.guid, &source_env), t)
                .ok_or_else(|| format!("no automation item at {} on {}", of.at, source_env.name))?;
            Some((pool, length))
        }
        None => None,
    };
    let to = match (&a.to, pooled) {
        (Some(to), _) => crate::fx_actions::moment(daw, &project, to)?,
        (None, Some((_, length))) => from + length,
        (None, None) => return Err("a fresh item needs `to`".to_owned()),
    };
    if to <= from {
        return Err(format!("{to} is not after {from}"));
    }
    let rest = current(daw, &project, &track.guid, &found, &at);
    let curve = curve_between(daw, &project, &at, (from, to), rest);
    let made = undoable(daw, &project, "Make an automation item", || {
        show(daw, &project, &track.guid, &found, 64);
        let span = (
            PositionInSeconds::from_seconds(from),
            Duration::from_seconds(to - from),
        );
        let made = Automation::add_automation_item(
            daw,
            project.clone(),
            at.clone(),
            match pooled {
                Some((pool, _)) => AddAutomationItemParams::pooled(pool, span.0, span.1),
                None => AddAutomationItemParams::new(span.0, span.1),
            },
        );
        if let (Ok(index), None) = (&made, pooled) {
            let _ = Automation::set_automation_item_points(
                daw,
                project.clone(),
                at.clone(),
                SetAutomationItemPointsParams {
                    index: *index,
                    points: curve.clone(),
                },
            );
        }
        made
    });
    let index = made.map_err(|e| e.to_string())?;
    Ok(match pooled {
        Some((pool, _)) => format!("item #{index} on {}: another of pool {pool}", found.name),
        None => format!(
            "item #{index} on {}: a fresh pool, {} points",
            found.name,
            curve.len()
        ),
    })
}

/// `AUTOMATION_WRITE`: points into an envelope, what was between the
/// first and the last replaced.
fn write(a: &WriteArg) -> Result<String, String> {
    let (daw, project) = crate::actions::song()?;
    let track = track_of(daw, &project, &a.track)?;
    let found = find(daw, &project, &track.guid, &a.envelope)?;
    let mut points = a
        .points
        .iter()
        .map(|p| {
            let t = crate::fx_actions::moment(daw, &project, &p.at)?;
            let value = engine_value(
                &found,
                &Value {
                    value: p.value,
                    text: p.text.clone(),
                },
            )?;
            let shape = match p.shape.as_deref() {
                Some(word) => crate::fx_actions::shape_of(Some(word))?,
                None if found.stepped() => EnvelopeShape::Square,
                None => found.kind.shape(),
            };
            Ok(point(t, value, shape))
        })
        .collect::<Result<Vec<_>, String>>()?;
    points.sort_by(|x, y| x.time.as_seconds().total_cmp(&y.time.as_seconds()));
    let span = (
        points.first().map_or(0.0, |p| p.time.as_seconds()),
        points.last().map_or(0.0, |p| p.time.as_seconds()),
    );
    undoable(daw, &project, "Write automation", || {
        show(daw, &project, &track.guid, &found, 64);
        put(daw, &project, &location(&track.guid, &found), span, &points);
    });
    Ok(format!(
        "{} points written to {} on {}",
        points.len(),
        found.name,
        track.name
    ))
}

pub(crate) fn register(list: &architect::action::ActionList) {
    use architect::action::Effect;
    list.register(&LANES, Effect::ReadOnly, |a: TrackArg| lanes(&a));
    list.register(&SHOW, Effect::Mutating, |a: ShowArg| {
        let (daw, project) = crate::actions::song()?;
        let track = track_of(daw, &project, &a.track)?;
        if a.envelope.trim().eq_ignore_ascii_case("item volume") {
            let shown = a.shown.unwrap_or(true);
            crate::automation_menu::show_takes(daw, &project, &track.guid, shown);
            crate::studio::request_resync();
            return Ok(format!(
                "Item volume curves on {}: {}",
                track.name,
                if shown { "shown" } else { "hidden" }
            ));
        }
        if a.envelope
            .trim()
            .eq_ignore_ascii_case(crate::automation::LEVELER)
            || a.envelope.trim().eq_ignore_ascii_case("leveler")
        {
            let shown = a.shown.unwrap_or(true);
            crate::vocal_level::show_lane(daw, &project, &track.guid, shown);
            crate::studio::request_resync();
            return Ok(format!(
                "{} on {}: {}",
                crate::automation::LEVELER,
                track.name,
                if shown { "shown" } else { "hidden" }
            ));
        }
        let found = find(daw, &project, &track.guid, &a.envelope)?;
        let shown = a.shown.unwrap_or(true);
        undoable(daw, &project, "Show an envelope", || {
            if shown {
                show(daw, &project, &track.guid, &found, a.height.unwrap_or(64));
            } else {
                Automation::set_visible(daw, project.clone(), location(&track.guid, &found), false);
            }
        });
        Ok::<_, String>(format!(
            "{} on {}: {}",
            found.name,
            track.name,
            if shown { "shown" } else { "hidden" }
        ))
    });
    list.register(&WRITE, Effect::Mutating, |a: WriteArg| write(&a));
    list.register(&CLEAR, Effect::Mutating, |a: ClearArg| {
        let (daw, project) = crate::actions::song()?;
        let track = track_of(daw, &project, &a.track)?;
        let found = find(daw, &project, &track.guid, &a.envelope)?;
        let at = location(&track.guid, &found);
        let (from, to) = (a.from.unwrap_or(0.0), a.to.unwrap_or(f64::MAX / 4.0));
        undoable(daw, &project, "Clear automation", || {
            put(daw, &project, &at, (from, to), &[]);
            let items = Automation::automation_items(daw, project.clone(), at.clone());
            for item in items.iter().rev() {
                let start = item.position.as_seconds();
                if start >= from && start < to {
                    let _ = Automation::delete_automation_item(
                        daw,
                        project.clone(),
                        at.clone(),
                        item.index,
                    );
                }
            }
        });
        Ok::<_, String>(format!("{} on {} cleared", found.name, track.name))
    });
    list.register(&MODE, Effect::Mutating, |a: ModeArg| {
        let (daw, project) = crate::actions::song()?;
        let track = track_of(daw, &project, &a.track)?;
        let found = find(daw, &project, &track.guid, &a.envelope)?;
        let mode = crate::fx_actions::mode_of(&a.mode)?;
        undoable(daw, &project, "Automation mode", || {
            Automation::set_automation_mode(
                daw,
                project.clone(),
                location(&track.guid, &found),
                mode,
            );
        });
        Ok::<_, String>(format!("{} on {}: {mode:?}", found.name, track.name))
    });
    list.register(&ITEM, Effect::Mutating, |a: ItemArg| item(&a));
    list.register(&SECTIONS, Effect::ReadOnly, |(): ()| {
        let (daw, project) = crate::actions::song()?;
        Ok::<_, String>(analyse(daw, &project))
    });
    list.register(&BY_SECTION, Effect::Mutating, |a: BySectionArg| {
        by_section(&a)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(name: &str, loud: bool, vocal: bool) -> Analysis {
        Analysis {
            name: name.to_owned(),
            start: 0.0,
            end: 1.0,
            level_db: 0.0,
            sung: 0.0,
            loud,
            vocal,
        }
    }

    #[test]
    fn a_section_is_picked_by_name_kind_or_what_is_in_it() {
        let chorus = section("CH 2", true, true);
        let solo = section("INST", true, false);
        let verse = section("VS 1", false, true);
        assert!(picks("CH 2", &chorus) && picks("ch", &chorus));
        assert!(!picks("CH 1", &chorus));
        assert!(picks("loud instrumental", &solo) && !picks("loud instrumental", &chorus));
        assert!(picks("quiet vocal", &verse) && picks("vocal", &chorus));
        assert!(!picks("bridge", &verse));
    }
}
