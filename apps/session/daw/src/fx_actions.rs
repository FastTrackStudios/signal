//! A track's FX over the Action List.
//!
//! Every processor on it, every parameter of each, read and set — in the
//! plugin's own words where the plugin is one of the FTS processors
//! (`-18 dB`, `Plate`, `1/2`), or as a normalized 0..1 for any plugin.
//!
//! An FX is named by its name in the chain (`FTS Reverb`, `Pro-Q 4`, any
//! part of it, case aside) or its place (`#2`, from 1); a parameter by its
//! name (`Decay`; a name a plugin repeats — the reverb's six `Post Freq` —
//! with its count: `Post Freq 6`) or its index (`#12`, from 0).
//!
//! And their automation: a parameter's envelope, its points placed at a
//! bar (`33:2`, bar 33 beat 2), a time (`12.5s`) or a section (`Chorus 1`,
//! `Chorus 1 end`), each point's value in the same words a setting takes.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Effects, FxChainContext, FxRef, FxTarget, ProjectContext, TempoMap as _};
use facet::Facet;

use crate::actions::{meta, resolve, song};

static LIST: ActionMeta = meta(
    "FX_LIST",
    "fx_list",
    "A track's FX",
    "The processors on `track` (its GUID, path or name), in chain order: each one's place (`#1`…), name, whether it is on, and how many parameters it has. Read-only.",
    "FX",
    "",
);

static PARAMS: ActionMeta = meta(
    "FX_PARAMS",
    "fx_params",
    "An FX's parameters",
    "Every parameter of one FX on `track` — `fx` its name (any part) or place (`#2`) — each with its index, name, its value as the plugin writes it (`-18.0 dB`, `Plate`) and normalized (0..1), and for a choice its choices. `filter` keeps the names containing it. Read-only.",
    "FX",
    "",
);

static SET: ActionMeta = meta(
    "FX_SET",
    "fx_set",
    "Set FX parameters",
    "Set parameters of one FX on `track` (`fx` its name or `#place`). `set` maps each parameter (name — `Post Freq 6` for the sixth of a repeated one — or `#index`) to a value: the plugin's own words for an FTS processor or any choice (`-18 dB`, `4:1`, `Plate`, `1/2`, `On`), or `=0.42` for a normalized value on any plugin. Says what each now reads, or why not. One undo step.",
    "FX",
    "",
);

static ENABLE: ActionMeta = meta(
    "FX_ENABLE",
    "fx_enable",
    "Switch an FX on or off",
    "Bypass (`on: false`) or un-bypass (`on: true`) one FX on `track` (`fx` its name or `#place`).",
    "FX",
    "",
);

static ADD: ActionMeta = meta(
    "FX_ADD",
    "fx_add",
    "Add an FX",
    "Put one of the FTS processors (`fx`: `FTS Mic`, `FTS EQ`, `FTS Comp`, …) on `track`, at place `at` in its chain (1 is first; the end when not given) — on, or bypassed with `on: false` — and `set` its parameters as FX_SET does, in the plugin's own words. Says its place and what each setting now reads. One undo step.",
    "FX",
    "",
);

static AUTOMATION: ActionMeta = meta(
    "FX_AUTOMATION",
    "fx_automation",
    "A track's plugin automation",
    "Every automated FX parameter on `track` (or only `fx`'s, its name or `#place`): the FX, the parameter, its mode, and its points — each at its bar:beat and seconds, its value normalized, and its shape. Read-only.",
    "FX",
    "",
);

static AUTOMATE: ActionMeta = meta(
    "FX_AUTOMATE",
    "fx_automate",
    "Automate an FX parameter",
    "Write points on one FX parameter's envelope: `track`, `fx` (name or `#place`), `param` (name, `Post Freq 6`, or `#index`), `points` — each `at` a bar (`33`, `33:2`, `33:2.5`), a time (`12.5s`) or a section (`Chorus 1`, `Chorus 1 end`), its `value` in FX_SET's words (`-18 dB`, `Plate`, `=0.4`), and optionally its `shape` to the next point (linear, step, smooth, fast start, fast end). The envelope's points between the first and last new one are replaced; `replace: true` clears the whole envelope first. `mode` sets how it plays (read, trim, touch, latch, write, off; read by default). Plays at once, saves with the song.",
    "FX",
    "",
);

static UNAUTOMATE: ActionMeta = meta(
    "FX_UNAUTOMATE",
    "fx_unautomate",
    "Remove FX parameter automation",
    "Delete one FX parameter's automation points on `track` (`fx`, `param` as FX_AUTOMATE) — between `from` and `to` (bars, times or sections as FX_AUTOMATE's `at`) or all of them — leaving the parameter at its set value where none are left.",
    "FX",
    "",
);

/// A track.
#[derive(Facet)]
struct TrackArg {
    track: String,
}

/// One FX on a track, its parameters narrowed by name.
#[derive(Facet)]
struct ParamsArg {
    track: String,
    fx: String,
    filter: Option<String>,
}

/// Parameters to set on one FX.
#[derive(Facet)]
struct SetArg {
    track: String,
    fx: String,
    set: std::collections::HashMap<String, String>,
}

/// An FX to put on a track.
#[derive(Facet)]
struct AddArg {
    track: String,
    fx: String,
    at: Option<u32>,
    on: Option<bool>,
    #[facet(default)]
    set: std::collections::HashMap<String, String>,
}

/// One FX on or off.
#[derive(Facet)]
struct EnableArg {
    track: String,
    fx: String,
    on: bool,
}

/// A track's automation, or one FX's.
#[derive(Facet)]
struct AutomationArg {
    track: String,
    fx: Option<String>,
}

/// One point to write.
#[derive(Facet)]
struct PointArg {
    at: String,
    value: String,
    shape: Option<String>,
}

/// Points to write on one parameter's envelope.
#[derive(Facet)]
struct AutomateArg {
    track: String,
    fx: String,
    param: String,
    points: Vec<PointArg>,
    replace: Option<bool>,
    mode: Option<String>,
}

/// One parameter's points to delete, between two moments or all.
#[derive(Facet)]
struct UnautomateArg {
    track: String,
    fx: String,
    param: String,
    from: Option<String>,
    to: Option<String>,
}

/// An automated parameter, as listed.
#[derive(Facet)]
struct EnvelopeRow {
    fx: String,
    param: String,
    mode: String,
    points: Vec<PointRow>,
}

/// One automation point, as listed.
#[derive(Facet)]
struct PointRow {
    bar: String,
    seconds: f64,
    value: f64,
    shape: String,
}

/// An FX in a chain, as listed.
#[derive(Facet)]
struct FxRow {
    place: String,
    name: String,
    on: bool,
    parameters: u32,
}

/// One parameter, as listed.
#[derive(Facet)]
struct ParamRow {
    index: u32,
    name: String,
    value: String,
    normalized: f64,
    choices: Vec<String>,
}

/// `track`'s chain and the FX `which` names in it.
pub(crate) fn target(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    which: &str,
) -> Result<(FxTarget, daw_proto::fx::Fx), String> {
    let guid = resolve(daw, project, track)?;
    let context = FxChainContext::Track(guid);
    let chain = Effects::list(daw, project.clone(), context.clone());
    let which = which.trim();
    let found = if let Some(place) = which.strip_prefix('#') {
        let place: usize = place
            .parse()
            .map_err(|_| format!("{which:?} is not a place"))?;
        chain
            .get(place.checked_sub(1).ok_or("places count from #1")?)
            .cloned()
    } else {
        let lower = which.to_lowercase();
        let named: Vec<&daw_proto::fx::Fx> = chain
            .iter()
            .filter(|fx| fx.name.to_lowercase().contains(&lower))
            .collect();
        match named.as_slice() {
            [] => None,
            [one] => Some((*one).clone()),
            many => {
                // An exact name wins over the others it is part of.
                many.iter()
                    .find(|fx| fx.name.eq_ignore_ascii_case(which))
                    .map(|fx| (*fx).clone())
                    .ok_or_else(|| {
                        format!(
                            "{which:?} could be any of {} — name one, or its #place",
                            many.iter()
                                .map(|fx| fx.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                    .map(Some)?
            }
        }
    };
    let fx = found.ok_or_else(|| {
        format!(
            "no {which:?} on the track — it has {}",
            chain
                .iter()
                .map(|fx| fx.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    Ok((
        FxTarget {
            context,
            fx: FxRef::Guid(fx.guid.clone()),
        },
        fx,
    ))
}

/// The parameter `which` names among `params`: `#index`, a name, or a
/// repeated name with its count (`Post Freq 6`).
pub(crate) fn param_of<'a>(
    params: &'a [daw_proto::fx::FxParameter],
    which: &str,
) -> Result<&'a daw_proto::fx::FxParameter, String> {
    let which = which.trim();
    if let Some(index) = which.strip_prefix('#') {
        let index: u32 = index
            .parse()
            .map_err(|_| format!("{which:?} is not an index"))?;
        return params
            .iter()
            .find(|p| p.index == index)
            .ok_or_else(|| format!("no parameter #{index}"));
    }
    let named: Vec<&daw_proto::fx::FxParameter> = params
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(which))
        .collect();
    if let [one] = named.as_slice() {
        return Ok(one);
    }
    if named.len() > 1 {
        return Err(format!(
            "{which:?} is {} parameters — say which: `{which} 1`…`{which} {}`",
            named.len(),
            named.len()
        ));
    }
    // A repeated name with its count.
    if let Some((base, n)) = which.rsplit_once(' ')
        && let Ok(n) = n.parse::<usize>()
    {
        let same: Vec<&daw_proto::fx::FxParameter> = params
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(base))
            .collect();
        if let Some(found) = n.checked_sub(1).and_then(|i| same.get(i)) {
            return Ok(found);
        }
    }
    Err(format!("no parameter {which:?} (FX_PARAMS lists them)"))
}

/// The normalized value for `text` on `param` of `fx`: `=0.42` as given;
/// a choice by its label; else the plugin's own reading of the words (an
/// FTS processor's, `builtin_fx::setting`).
pub(crate) fn value_of(
    fx: &str,
    param: &daw_proto::fx::FxParameter,
    text: &str,
) -> Result<f64, String> {
    let text = text.trim();
    if let Some(raw) = text.strip_prefix('=') {
        let v: f64 = raw
            .trim()
            .parse()
            .map_err(|_| format!("{text:?} is not a number"))?;
        return Ok(v.clamp(0.0, 1.0));
    }
    if let Some((v, _)) = param
        .step_labels
        .iter()
        .find(|(_, label)| label.eq_ignore_ascii_case(text))
    {
        return Ok(*v);
    }
    // The FTS processors read their own words; the index found by name is
    // the first of a repeated one, so the value is the same either way.
    builtin_fx::setting(fx, &param.name, text)
        .map(|(_, v)| v)
        .ok_or_else(|| {
            format!(
                "{} does not read {text:?} for {} — use its own words, or `=0..1`",
                fx, param.name
            )
        })
}

/// Where `text` is, seconds: a bar (`33`, `33:2`, `33:2.5` — beat 2 and a
/// half), a time (`12.5s`), or a section by name (`Chorus 1`; its end with
/// ` end`).
pub(crate) fn moment(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    text: &str,
) -> Result<f64, String> {
    let text = text.trim();
    if let Some(secs) = text.strip_suffix('s')
        && let Ok(secs) = secs.trim().parse::<f64>()
    {
        return Ok(secs.max(0.0));
    }
    let (bar, beat) = text.split_once(':').unwrap_or((text, "1"));
    if let (Ok(bar), Ok(beat)) = (bar.trim().parse::<i32>(), beat.trim().parse::<f64>()) {
        use num_traits::ToPrimitive as _;
        let whole = beat.floor().max(1.0);
        let into = beat - whole;
        let beat = whole.to_i32().ok_or_else(|| format!("no beat {whole}"))?;
        // The beat's start, then the fraction of it on.
        // The bar as the song counts it.
        let bar = crate::song_bars::measure(daw, &project, bar);
        let start = daw.musical_to_time(project.clone(), bar, beat, 0.0);
        let next = daw.musical_to_time(project.clone(), bar, beat.saturating_add(1), 0.0);
        return Ok(into.mul_add(next - start, start));
    }
    let (name, end) = text
        .strip_suffix(" end")
        .map_or((text, false), |name| (name, true));
    let sections = crate::actions::sections(daw, project);
    sections
        .iter()
        .find(|s| s.name.trim().eq_ignore_ascii_case(name.trim()))
        .map(|s| if end { s.end } else { s.start })
        .ok_or_else(|| {
            format!(
                "{text:?} is not a bar (`33:2`), a time (`12.5s`) or a section — the sections are {}",
                sections
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The shape a word names.
pub(crate) fn shape_of(
    word: Option<&str>,
) -> Result<daw::service::automation::EnvelopeShape, String> {
    use daw::service::automation::EnvelopeShape;
    match word.map(|w| w.trim().to_lowercase()).as_deref() {
        None | Some("" | "linear" | "line" | "ramp") => Ok(EnvelopeShape::Linear),
        Some("step" | "square" | "jump") => Ok(EnvelopeShape::Square),
        Some("smooth" | "s-curve" | "slow start end") => Ok(EnvelopeShape::SlowStartEnd),
        Some("fast start") => Ok(EnvelopeShape::FastStart),
        Some("fast end") => Ok(EnvelopeShape::FastEnd),
        Some("bezier") => Ok(EnvelopeShape::Bezier),
        Some(other) => Err(format!(
            "no shape {other:?} — linear, step, smooth, fast start, fast end"
        )),
    }
}

/// A shape's word.
const fn shape_word(shape: daw::service::automation::EnvelopeShape) -> &'static str {
    use daw::service::automation::EnvelopeShape;
    match shape {
        EnvelopeShape::Linear => "linear",
        EnvelopeShape::Square => "step",
        EnvelopeShape::SlowStartEnd => "smooth",
        EnvelopeShape::FastStart => "fast start",
        EnvelopeShape::FastEnd => "fast end",
        EnvelopeShape::Bezier => "bezier",
    }
}

/// The mode a word names.
pub(crate) fn mode_of(word: &str) -> Result<daw::service::AutomationMode, String> {
    use daw::service::AutomationMode;
    match word.trim().to_lowercase().as_str() {
        "read" => Ok(AutomationMode::Read),
        "trim" | "trim/read" => Ok(AutomationMode::TrimRead),
        "touch" => Ok(AutomationMode::Touch),
        "latch" => Ok(AutomationMode::Latch),
        "latch preview" => Ok(AutomationMode::LatchPreview),
        "write" => Ok(AutomationMode::Write),
        "off" => Ok(AutomationMode::Off),
        other => Err(format!(
            "no mode {other:?} — read, trim, touch, latch, write, off"
        )),
    }
}

/// A mode's word.
const fn mode_word(mode: daw::service::AutomationMode) -> &'static str {
    use daw::service::AutomationMode;
    match mode {
        AutomationMode::Read => "read",
        AutomationMode::TrimRead => "trim",
        AutomationMode::Touch => "touch",
        AutomationMode::Latch => "latch",
        AutomationMode::LatchPreview => "latch preview",
        AutomationMode::Write => "write",
        AutomationMode::Off => "off",
    }
}

/// Where a parameter's envelope is.
fn envelope_at(
    target: &FxTarget,
    track: &str,
    index: u32,
) -> Result<daw::service::automation::EnvelopeLocation, String> {
    let FxRef::Guid(fx_guid) = &target.fx else {
        return Err("the FX has no guid".to_owned());
    };
    Ok(daw::service::automation::EnvelopeLocation::new(
        daw::service::TrackRef::Guid(track.to_owned()),
        daw::service::automation::EnvelopeRef::FxParam {
            fx_guid: fx_guid.clone(),
            param_index: index,
        },
    ))
}

/// The track a target's chain is on.
fn track_of(target: &FxTarget) -> Result<&str, String> {
    match &target.context {
        FxChainContext::Track(guid) => Ok(guid),
        _ => Err("not a track's chain".to_owned()),
    }
}

/// Delete `at`'s points from `from` to `to`, seconds.
fn clear(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    at: &daw::service::automation::EnvelopeLocation,
    (from, to): (f64, f64),
) {
    daw::service::Automation::delete_points_in_range(
        daw,
        project.clone(),
        at.clone(),
        daw::service::automation::TimeRangeParams {
            start: daw::service::PositionInSeconds::from_seconds(from),
            end: daw::service::PositionInSeconds::from_seconds(to),
        },
    );
}

/// `FX_AUTOMATION`: the automated parameters on a track, or on one FX.
fn automation(a: &AutomationArg) -> Result<Vec<EnvelopeRow>, String> {
    let (daw, project) = song()?;
    let guid = resolve(daw, &project, &a.track)?;
    let context = FxChainContext::Track(guid.clone());
    let chain: Vec<daw_proto::fx::Fx> = match &a.fx {
        Some(which) => vec![target(daw, &project, &a.track, which)?.1],
        None => Effects::list(daw, project.clone(), context.clone()),
    };
    let envelopes = daw::service::Automation::envelopes(
        daw,
        project.clone(),
        daw::service::TrackRef::Guid(guid.clone()),
    );
    let mut rows = Vec::new();
    for fx in &chain {
        let fx_target = FxTarget {
            context: context.clone(),
            fx: FxRef::Guid(fx.guid.clone()),
        };
        let params = Effects::parameters(daw, project.clone(), fx_target.clone());
        for envelope in &envelopes {
            let (Some(fx_guid), Some(param_index)) = (&envelope.fx_guid, &envelope.param_index)
            else {
                continue;
            };
            if fx_guid != &fx.guid {
                continue;
            }
            let at = envelope_at(&fx_target, &guid, *param_index)?;
            let points = daw::service::Automation::points(daw, project.clone(), at);
            if points.is_empty() {
                continue;
            }
            rows.push(EnvelopeRow {
                fx: fx.name.clone(),
                param: params
                    .iter()
                    .find(|p| p.index == *param_index)
                    .map_or_else(|| format!("#{param_index}"), |p| p.name.clone()),
                mode: mode_word(envelope.automation_mode).to_owned(),
                points: points
                    .iter()
                    .map(|pt| {
                        let secs = pt.time.as_seconds();
                        let (bar, beat, frac) = crate::song_bars::musical(daw, &project, secs);
                        PointRow {
                            bar: format!("{bar}:{}", f64::from(beat) + frac),
                            seconds: secs,
                            value: pt.value,
                            shape: shape_word(pt.shape).to_owned(),
                        }
                    })
                    .collect(),
            });
        }
    }
    Ok(rows)
}

/// `FX_AUTOMATE`: write points on one parameter's envelope.
fn automate(a: &AutomateArg) -> Result<String, String> {
    let (daw, project) = song()?;
    let (target, fx) = target(daw, &project, &a.track, &a.fx)?;
    let params = Effects::parameters(daw, project.clone(), target.clone());
    let param = param_of(&params, &a.param)?;
    let mut points = a
        .points
        .iter()
        .map(|p| {
            Ok((
                moment(daw, &project, &p.at)?,
                value_of(&fx.name, param, &p.value)?,
                shape_of(p.shape.as_deref())?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    points.sort_by(|x, y| x.0.total_cmp(&y.0));
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Err("no points".to_owned());
    };
    let mode = mode_of(a.mode.as_deref().unwrap_or("read"))?;
    let at = envelope_at(&target, track_of(&target)?, param.index)?;
    let span = if a.replace.unwrap_or(false) {
        (0.0, f64::MAX)
    } else {
        // A hair either side: a point exactly on the first or last is
        // replaced too.
        (first.0 - 1e-6, last.0 + 1e-6)
    };
    clear(daw, &project, &at, span);
    for (time, value, shape) in &points {
        let _ = daw::service::Automation::add_point(
            daw,
            project.clone(),
            at.clone(),
            daw::service::automation::AddPointParams::new(
                daw::service::PositionInSeconds::from_seconds(*time),
                *value,
                *shape,
            ),
        );
    }
    daw::service::Automation::set_automation_mode(daw, project.clone(), at.clone(), mode);
    crate::actions::list().changed("fx", &fx.guid);
    Ok(format!(
        "{} {}: {} points, {} — now {} in all",
        fx.name,
        a.param.trim(),
        points.len(),
        mode_word(mode),
        daw::service::Automation::points(daw, project, at).len()
    ))
}

/// `FX_UNAUTOMATE`: delete one parameter's points, in a span or all.
fn unautomate(a: &UnautomateArg) -> Result<String, String> {
    let (daw, project) = song()?;
    let (target, fx) = target(daw, &project, &a.track, &a.fx)?;
    let params = Effects::parameters(daw, project.clone(), target.clone());
    let param = param_of(&params, &a.param)?;
    let at = envelope_at(&target, track_of(&target)?, param.index)?;
    let from = a
        .from
        .as_deref()
        .map(|t| moment(daw, &project, t))
        .transpose()?
        .unwrap_or(0.0);
    let to =
        a.to.as_deref()
            .map(|t| moment(daw, &project, t))
            .transpose()?
            .unwrap_or(f64::MAX);
    let before = daw::service::Automation::points(daw, project.clone(), at.clone()).len();
    clear(daw, &project, &at, (from, to));
    let after = daw::service::Automation::points(daw, project, at).len();
    crate::actions::list().changed("fx", &fx.guid);
    Ok(format!(
        "{} {}: {} points removed, {after} left",
        fx.name,
        a.param.trim(),
        before.saturating_sub(after)
    ))
}

/// `set` (parameter → the plugin's words) on the FX `target` (named
/// `name`), in name order: what each now reads, or why not.
fn settle(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (target, name): (&FxTarget, &str),
    set: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let params = Effects::parameters(daw, project.clone(), target.clone());
    let mut settings: Vec<(&String, &String)> = set.iter().collect();
    settings.sort();
    settings
        .into_iter()
        .map(|(which, text)| {
            param_of(&params, which)
                .and_then(|param| {
                    let value = value_of(name, param, text)?;
                    Effects::set_parameter(
                        daw,
                        project.clone(),
                        daw_proto::fx::SetParameterRequest {
                            target: target.clone(),
                            index: param.index,
                            value,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    let now = Effects::parameter(daw, project.clone(), target.clone(), param.index)
                        .map_or_else(String::new, |p| p.formatted);
                    // As asked: `Post Freq 6`, not the name all six share.
                    Ok(format!("{} = {now}", which.trim()))
                })
                .unwrap_or_else(|e| format!("{which}: {e}"))
        })
        .collect()
}

pub(crate) fn register(list: &ActionList) {
    list.register(&LIST, Effect::ReadOnly, |a: TrackArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let context = FxChainContext::Track(guid);
        Ok(Effects::list(daw, project.clone(), context.clone())
            .into_iter()
            .enumerate()
            .map(|(i, fx)| FxRow {
                place: format!("#{}", i.saturating_add(1)),
                parameters: u32::try_from(
                    Effects::parameters(
                        daw,
                        project.clone(),
                        FxTarget {
                            context: context.clone(),
                            fx: FxRef::Guid(fx.guid.clone()),
                        },
                    )
                    .len(),
                )
                .unwrap_or(u32::MAX),
                name: fx.name,
                on: fx.enabled,
            })
            .collect::<Vec<_>>())
    });
    list.register(&PARAMS, Effect::ReadOnly, |a: ParamsArg| {
        let (daw, project) = song()?;
        let (target, _) = target(daw, &project, &a.track, &a.fx)?;
        let filter = a.filter.map(|f| f.to_lowercase());
        Ok(Effects::parameters(daw, project, target)
            .into_iter()
            .filter(|p| {
                filter
                    .as_ref()
                    .is_none_or(|f| p.name.to_lowercase().contains(f))
            })
            .map(|p| ParamRow {
                index: p.index,
                choices: p.step_labels.iter().map(|(_, l)| l.clone()).collect(),
                name: p.name,
                value: p.formatted,
                normalized: p.value,
            })
            .collect::<Vec<_>>())
    });
    list.register(&SET, Effect::Mutating, |a: SetArg| {
        let (daw, project) = song()?;
        let (target, fx) = target(daw, &project, &a.track, &a.fx)?;
        let said = settle(daw, &project, (&target, &fx.name), &a.set);
        crate::actions::list().changed("fx", &fx.guid);
        Ok(said)
    });
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = song()?;
        if !daw_standalone::plugin::FxFactory::provides(&builtin_fx::BuiltinFx, &a.fx) {
            return Err(format!(
                "{} is not one of the FTS processors (FTS Mic, FTS EQ, FTS Comp, …)",
                a.fx
            ));
        }
        let guid = resolve(daw, &project, &a.track)?;
        let context = FxChainContext::Track(guid);
        let index = a.at.unwrap_or(u32::MAX).saturating_sub(1);
        let label = format!("Add {}", a.fx);
        daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
        let added = Effects::add_at(
            daw,
            project.clone(),
            daw_proto::fx::AddFxAtRequest {
                context: context.clone(),
                name: a.fx.clone(),
                index,
            },
        );
        let out = added
            .ok_or_else(|| format!("{} could not be added", a.fx))
            .and_then(|fx_guid| {
                let target = FxTarget {
                    context: context.clone(),
                    fx: FxRef::Guid(fx_guid.clone()),
                };
                if !a.on.unwrap_or(true) {
                    Effects::set_enabled(daw, project.clone(), target.clone(), false)
                        .map_err(|e| e.to_string())?;
                }
                let mut said = settle(daw, &project, (&target, &a.fx), &a.set);
                let place = Effects::list(daw, project.clone(), context.clone())
                    .iter()
                    .position(|f| f.guid == fx_guid)
                    .map_or(0, |i| i.saturating_add(1));
                said.insert(
                    0,
                    format!(
                        "{} at #{place}{}",
                        a.fx,
                        if a.on.unwrap_or(true) {
                            ""
                        } else {
                            ", bypassed"
                        }
                    ),
                );
                crate::actions::list().changed("fx", &fx_guid);
                Ok(said)
            });
        daw::service::Projects::end_undo_block(daw, project, &label, None);
        out
    });
    list.register(&AUTOMATION, Effect::ReadOnly, |a: AutomationArg| {
        automation(&a)
    });
    list.register(&AUTOMATE, Effect::Mutating, |a: AutomateArg| automate(&a));
    list.register(&UNAUTOMATE, Effect::Mutating, |a: UnautomateArg| {
        unautomate(&a)
    });
    list.register(&ENABLE, Effect::Mutating, |a: EnableArg| {
        let (daw, project) = song()?;
        let (target, fx) = target(daw, &project, &a.track, &a.fx)?;
        Effects::set_enabled(daw, project, target, a.on).map_err(|e| e.to_string())?;
        Ok(format!(
            "{} {}",
            fx.name,
            if a.on { "on" } else { "bypassed" }
        ))
    });
}
