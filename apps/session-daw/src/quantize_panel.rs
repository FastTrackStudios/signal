//! The Quantize panel: the drum quantize as a hand runs it.
//!
//! Over the arrangement's right, opened from the bottom bar's Quantize:
//! a section (or the time selection), its groove as `DRUMS_GROOVE` reads
//! it, and the quantize's settings — grid, what places a hit (`Pattern`:
//! the section's own groove; `Guided`: the drum MIDI; `Grid`), strength,
//! the kick locked for a double kick, the fills — then **Preview**, a dry
//! run that says what would move, and **Apply**, which edits the kit and
//! says how its check came out. Every press is the `QUANTIZE` a model
//! runs, so the panel and a model can never quantize two ways.
//!
//! What Apply used is kept: `q` quantizes the kit rows selected with it
//! ([`again_over`]), and the sticky settings take it too, so it outlives
//! a restart — all but a locked kick, which is this run's alone.

use dioxus::prelude::*;
use serde_json::Value;

/// The panel's choices.
#[derive(Clone, Debug, PartialEq)]
pub struct Choices {
    /// The section's name, or `None` for the time selection.
    pub section: Option<String>,
    /// The time selection, seconds, when there is one.
    pub selection: Option<(f64, f64)>,
    /// A whole note's division: 8, 16, 32, 12 (8th triplets), 24.
    pub grid: u32,
    pub target: &'static str,
    /// Percent.
    pub strength: u32,
    /// The kick locked in order to the grid (a double kick).
    pub lock_kick: bool,
    pub fills: &'static str,
}

impl Default for Choices {
    fn default() -> Self {
        Self {
            section: None,
            selection: None,
            grid: 16,
            target: "Pattern",
            strength: 100,
            lock_kick: false,
            fills: "Skip",
        }
    }
}

/// The `QUANTIZE` arguments for `choices`, a dry run or not; `None` with
/// nothing to quantize over.
#[must_use]
pub fn quantize_args(choices: &Choices, dry_run: bool) -> Option<String> {
    let mut args = serde_json::Map::new();
    match (&choices.section, choices.selection) {
        (Some(name), _) => {
            args.insert("sections".into(), Value::from(vec![name.clone()]));
        }
        (None, Some((from, to))) => {
            args.insert("from".into(), Value::from(from));
            args.insert("to".into(), Value::from(to));
        }
        (None, None) => return None,
    }
    args.insert("grid".into(), Value::from(choices.grid));
    args.insert(
        "strength".into(),
        Value::from(f64::from(choices.strength.min(100)) / 100.0),
    );
    args.insert("fills".into(), Value::from(choices.fills));
    if choices.lock_kick {
        args.insert("lock".into(), Value::from("Kick"));
    } else {
        args.insert("target".into(), Value::from(choices.target));
    }
    args.insert("dry_run".into(), Value::from(dry_run));
    args.insert("brief".into(), Value::from(true));
    Some(Value::Object(args).to_string())
}

/// The choices Apply last used: what `q` quantizes with.
static LAST: std::sync::Mutex<Option<Choices>> = std::sync::Mutex::new(None);

/// What Apply last used, if it has been pressed since the app started.
#[must_use]
pub fn last() -> Option<Choices> {
    LAST.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn remember(choices: &Choices) {
    *LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(choices.clone());
}

/// The `QUANTIZE` arguments for `q` over `from`..`to` seconds: with the
/// choices Apply last used, else the sticky settings as they stand.
#[must_use]
pub fn again_over(last: Option<&Choices>, (from, to): (f64, f64)) -> String {
    last.and_then(|c| {
        quantize_args(
            &Choices {
                section: None,
                selection: Some((from, to)),
                ..c.clone()
            },
            false,
        )
    })
    .unwrap_or_else(|| serde_json::json!({ "from": from, "to": to, "brief": true }).to_string())
}

/// The `DRUMS_EDIT_SET` arguments that make `choices` the sticky
/// settings (a locked kick is not one).
#[must_use]
pub fn sticky_args(choices: &Choices) -> String {
    serde_json::json!({
        "grid": choices.grid,
        "target": choices.target,
        "strength": f64::from(choices.strength.min(100)) / 100.0,
        "fills": choices.fills,
    })
    .to_string()
}

/// `choices` with the sticky settings `settings` (a `DRUMS_EDIT_SETTINGS`
/// answer) put in: those the panel offers, others left as they were.
#[must_use]
pub fn from_sticky(mut choices: Choices, settings: &Value) -> Choices {
    if let Some(grid) = settings
        .get("grid")
        .and_then(Value::as_u64)
        .and_then(|g| u32::try_from(g).ok())
    {
        choices.grid = grid;
    }
    let pick = |key: &str, from: &[&'static str]| {
        let given = settings.get(key).and_then(Value::as_str)?;
        from.iter().copied().find(|v| *v == given)
    };
    if let Some(target) = pick("target", &["Pattern", "Guided", "Grid"]) {
        choices.target = target;
    }
    if let Some(fills) = pick("fills", &["Skip", "Include", "Only"]) {
        choices.fills = fills;
    }
    if let Some(strength) = settings.get("strength").and_then(Value::as_f64) {
        let percent = (strength.clamp(0.0, 1.0) * 100.0).round();
        choices.strength = num_traits::ToPrimitive::to_u32(&percent).unwrap_or(100);
    }
    choices
}

/// What a `QUANTIZE` answer says, a line a span: how many hits move,
/// stay, are left; the groove's calls; how the check came out.
#[must_use]
pub fn summary(answer: &Value) -> Vec<String> {
    let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    answer
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|span| {
            let name = span.get("name").and_then(Value::as_str).unwrap_or("");
            let result = span.get("result").unwrap_or(&Value::Null);
            let calls = span
                .get("by_groove")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let mut lines = vec![format!(
                "{name}: {} move, {} on the line, {} left as played, {} in fills{}",
                n(result, "moved"),
                n(result, "in_place"),
                n(result, "unmatched"),
                n(result, "in_fills"),
                if calls > 0 {
                    format!(" — {calls} placed by the groove, not the nearest line")
                } else {
                    String::new()
                },
            )];
            if let Some(check) = span.get("check").filter(|c| !c.is_null()) {
                if check.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                    lines.push(
                        "checked: nothing lost, nothing doubled, every hit on its line".into(),
                    );
                } else {
                    lines.push(format!(
                        "check failed: {} lost, {} doubled, {} off target, {} moved outside",
                        n(check, "lost"),
                        n(check, "doubled"),
                        n(check, "off_target"),
                        n(check, "strayed"),
                    ));
                    lines.extend(
                        check
                            .get("problems")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .take(3)
                            .filter_map(Value::as_str)
                            .map(|p| format!("  {p}")),
                    );
                }
            }
            lines
        })
        .collect()
}

/// A `DRUMS_GROOVE` answer as lines: each piece's slots, counted as a
/// drummer counts them, and how much of the section they place.
#[must_use]
pub fn groove_lines(answer: &Value) -> Vec<String> {
    let Some(first) = answer.as_array().and_then(|a| a.first()) else {
        return Vec::new();
    };
    let none = first
        .get("pieces")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty);
    if none {
        return vec!["no pattern: no piece plays the same in most bars".into()];
    }
    let cycle = first.get("cycle_bars").and_then(Value::as_u64).unwrap_or(1);
    let placed = first.get("placed").and_then(Value::as_f64).unwrap_or(0.0);
    let mut lines = vec![format!(
        "{} cycle, {:.0}% of its hits in the pattern",
        if cycle > 1 { "two-bar" } else { "one-bar" },
        placed * 100.0
    )];
    lines.extend(
        first
            .get("pieces")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|piece| {
                let name = piece.get("piece").and_then(Value::as_str).unwrap_or("");
                let slots: Vec<&str> = piece
                    .get("slots")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.get("at").and_then(Value::as_str))
                    .collect();
                format!("{name}: {}", slots.join(" "))
            }),
    );
    lines
}

/// The `DRUMS_REVERT` arguments for the stretch `choices` names; `None`
/// with nothing chosen.
#[must_use]
pub fn revert_args(choices: &Choices) -> Option<String> {
    match (&choices.section, choices.selection) {
        (Some(name), _) => Some(serde_json::json!({ "sections": [name] }).to_string()),
        (None, Some((from, to))) => Some(serde_json::json!({ "from": from, "to": to }).to_string()),
        (None, None) => None,
    }
}

/// The leveling's knobs as the panel turns them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leveling {
    /// Percent: 100 brings every stroke to one level.
    pub consistency: u32,
    /// The most a stroke moves, dB.
    pub range_db: u32,
    /// Toward the drum's level over the whole song, not the playing
    /// around each stroke.
    pub whole_song: bool,
}

impl Default for Leveling {
    fn default() -> Self {
        Self {
            consistency: 75,
            range_db: 6,
            whole_song: false,
        }
    }
}

/// The `DRUMS_LEVEL` arguments for `leveling`.
#[must_use]
pub fn level_args(leveling: &Leveling) -> String {
    serde_json::json!({
        "consistency": leveling.consistency.min(100),
        "range_db": leveling.range_db,
        "whole_song": leveling.whole_song,
    })
    .to_string()
}

/// A `DRUMS_LEVEL` answer as lines, a drum each.
#[must_use]
pub fn level_lines(answer: &Value) -> Vec<String> {
    let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    answer
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            format!(
                "{}: {} strokes, {} up, {} down, at most {:+.1} dB",
                d.get("piece").and_then(Value::as_str).unwrap_or(""),
                n(d, "strokes"),
                n(d, "raised"),
                n(d, "lowered"),
                d.get("most_db").and_then(Value::as_f64).unwrap_or(0.0),
            )
        })
        .collect()
}

/// A show-or-hide asked for from outside the shell (`SESSION_QUANTIZE_PANEL`
/// on the Action List): the arrangement panel takes it on its next frame
/// and turns the shell's pin.
static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ask for the panel to be shown, or hidden if it is.
pub fn ask() {
    ASKED.store(true, std::sync::atomic::Ordering::Release);
}

/// Whether it is asked for and not yet taken: a frame is owed.
pub fn asked() -> bool {
    ASKED.load(std::sync::atomic::Ordering::Acquire)
}

/// Whether it was asked for since last taken.
pub fn take_asked() -> bool {
    ASKED.swap(false, std::sync::atomic::Ordering::AcqRel)
}

/// Run `id` with `args` on the Action List, off the window's thread.
async fn run(id: &'static str, args: String) -> Result<Value, String> {
    let runtime = crate::open::runtime().ok_or("no engine")?;
    let out = runtime
        .spawn_blocking(move || crate::actions::list().run_json(id, args.as_bytes()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&out).map_err(|e| e.to_string())
}

const CARD: &str = "position:absolute; top:12px; right:12px; width:340px; max-height:calc(100% - 24px); \
                    overflow:auto; box-sizing:border-box; padding:12px; border-radius:10px; \
                    background:#17191d; border:1px solid #2a2d33; color:#e6e6e6; \
                    font-size:12px; line-height:16px; display:flex; flex-direction:column; gap:10px;";
const ROW: &str = "display:flex; flex-wrap:wrap; gap:4px; align-items:center;";
const HEAD: &str = "font-size:10px; font-weight:700; color:#8a8f98; letter-spacing:0.04em;";

fn chip(on: bool) -> String {
    let (bg, fg) = if on {
        ("#3a6df0", "#ffffff")
    } else {
        ("#24272c", "#c9ccd1")
    };
    format!(
        "min-height:26px; padding:0 9px; border:none; border-radius:6px; background:{bg}; \
         color:{fg}; font-family:inherit; font-size:12px; cursor:pointer;"
    )
}

/// The panel. No props: the song is the open one.
#[component]
pub fn QuantizePanel() -> Element {
    let mut choices = use_signal(|| last().unwrap_or_default());
    let mut sections = use_signal(Vec::<String>::new);
    let mut said = use_signal(Vec::<String>::new);
    let mut groove = use_signal(Vec::<String>::new);
    let mut busy = use_signal(|| false);
    let mut leveling = use_signal(Leveling::default);

    // The song's sections, and the one under the edit cursor to start;
    // the settings `q` would use, when Apply has not been pressed yet.
    use_future(move || async move {
        if last().is_none() {
            if let Ok(settings) = run("DRUMS_EDIT_SETTINGS", "{}".into()).await {
                choices.with_mut(|c| *c = from_sticky(c.clone(), &settings));
            }
        }
        let Ok(found) = run("SESSION_SECTIONS", "{}".into()).await else {
            return;
        };
        let at = crate::cursor::current();
        let all: Vec<(String, f64, f64)> = found
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| {
                Some((
                    s.get("name")?.as_str()?.to_owned(),
                    s.get("start")?.as_f64()?,
                    s.get("end")?.as_f64()?,
                ))
            })
            .collect();
        let under = at.and_then(|e| {
            all.iter()
                .find(|(_, from, to)| e.at >= *from && e.at < *to)
                .map(|(n, _, _)| n.clone())
        });
        sections.set(all.iter().map(|(n, _, _)| n.clone()).collect());
        choices.with_mut(|c| {
            c.selection = at.and_then(|e| e.selection).map(|s| (s.start, s.end));
            c.section = under.or_else(|| all.first().map(|(n, _, _)| n.clone()));
        });
    });

    // The groove of the section chosen, read again when it changes.
    let section = choices.read().section.clone();
    use_effect(use_reactive!(|section| {
        spawn(async move {
            let Some(name) = section else {
                groove.set(Vec::new());
                return;
            };
            let args = serde_json::json!({ "sections": [name] }).to_string();
            groove.set(match run("DRUMS_GROOVE", args).await {
                Ok(answer) => groove_lines(&answer),
                Err(why) => vec![why],
            });
        });
    }));

    let mut go = move |dry: bool| {
        let Some(args) = quantize_args(&choices.read(), dry) else {
            said.set(vec!["Choose a section, or select a stretch of time".into()]);
            return;
        };
        busy.set(true);
        let keep = (!dry).then(|| choices.read().clone());
        said.set(vec![
            if dry {
                "Previewing…"
            } else {
                "Quantizing…"
            }
            .into(),
        ]);
        spawn(async move {
            if let Some(kept) = keep {
                remember(&kept);
                if let Err(why) = run("DRUMS_EDIT_SET", sticky_args(&kept)).await {
                    tracing::warn!(quantize.sticky_error = %why, "drums: the panel's settings were not kept");
                }
            }
            said.set(match run("QUANTIZE", args).await {
                Ok(answer) => summary(&answer),
                Err(why) => vec![why],
            });
            busy.set(false);
        });
    };

    let c = choices.read().clone();
    let lv = *leveling.read();
    let has_selection = c.selection.is_some();
    rsx! {
        div {
            style: CARD,
            onpointerdown: move |e| e.stop_propagation(),
            div { style: "font-size:14px; font-weight:700;", "Quantize drums" }
            div { style: HEAD, "WHERE" }
            div {
                style: ROW,
                if has_selection {
                    button {
                        style: chip(c.section.is_none()),
                        onclick: move |_| choices.with_mut(|c| c.section = None),
                        "Selection"
                    }
                }
                for name in sections.read().iter().cloned() {
                    button {
                        key: "{name}",
                        style: chip(c.section.as_deref() == Some(name.as_str())),
                        onclick: {
                            let pick = Some(name);
                            move |_| choices.with_mut(|c| c.section.clone_from(&pick))
                        },
                        "{name}"
                    }
                }
            }
            if !groove.read().is_empty() {
                div { style: HEAD, "ITS GROOVE" }
                div {
                    style: "display:flex; flex-direction:column; gap:2px; color:#c9ccd1;",
                    for line in groove.read().iter().cloned() {
                        div { "{line}" }
                    }
                }
            }
            div { style: HEAD, "GRID" }
            div {
                style: ROW,
                for (grid, name) in [(8_u32, "1/8"), (16, "1/16"), (32, "1/32"), (12, "1/8T"), (24, "1/16T")] {
                    button {
                        key: "{grid}",
                        style: chip(c.grid == grid),
                        onclick: move |_| choices.with_mut(|c| c.grid = grid),
                        "{name}"
                    }
                }
            }
            div { style: HEAD, "WHAT PLACES A HIT" }
            div {
                style: ROW,
                for (target, name) in [("Pattern", "Its groove"), ("Guided", "The drum MIDI"), ("Grid", "The nearest line")] {
                    button {
                        key: "{target}",
                        style: chip(!c.lock_kick && c.target == target),
                        onclick: move |_| choices.with_mut(|c| {
                            c.target = target;
                            c.lock_kick = false;
                        }),
                        "{name}"
                    }
                }
                button {
                    style: chip(c.lock_kick),
                    title: "A double kick: its hits locked in order to the grid, the kit with them",
                    onclick: move |_| choices.with_mut(|c| c.lock_kick = !c.lock_kick),
                    "Kick locked"
                }
            }
            div { style: HEAD, "STRENGTH" }
            div {
                style: ROW,
                for strength in [50_u32, 75, 90, 100] {
                    button {
                        key: "{strength}",
                        style: chip(c.strength == strength),
                        onclick: move |_| choices.with_mut(|c| c.strength = strength),
                        "{strength}%"
                    }
                }
            }
            div { style: HEAD, "FILLS" }
            div {
                style: ROW,
                for (fills, name) in [("Skip", "Left as played"), ("Include", "Edited too"), ("Only", "Only the fills")] {
                    button {
                        key: "{fills}",
                        style: chip(c.fills == fills),
                        onclick: move |_| choices.with_mut(|c| c.fills = fills),
                        "{name}"
                    }
                }
            }
            div {
                style: "display:flex; gap:6px;",
                button {
                    style: chip(false),
                    disabled: busy(),
                    onclick: move |_| go(true),
                    "Preview"
                }
                button {
                    style: chip(true),
                    disabled: busy(),
                    onclick: move |_| go(false),
                    "Apply"
                }
                button {
                    style: chip(false),
                    disabled: busy(),
                    title: "Undo the edit here: every hit back where it was played, the hit markers kept to drag by hand (Alt+Q)",
                    onclick: move |_| {
                        let Some(args) = revert_args(&choices.read()) else {
                            said.set(vec!["Choose a section, or select a stretch of time".into()]);
                            return;
                        };
                        busy.set(true);
                        said.set(vec!["Putting it back as played…".into()]);
                        spawn(async move {
                            said.set(match run("DRUMS_REVERT", args).await {
                                Ok(answer) => summary(&answer),
                                Err(why) => vec![why],
                            });
                            busy.set(false);
                        });
                    },
                    "Put back as played"
                }
            }
            div { style: HEAD, "LEVEL — HOW CONSISTENT EACH DRUM'S STROKES" }
            div {
                style: ROW,
                for percent in [0_u32, 25, 50, 75, 90, 100] {
                    button {
                        key: "{percent}",
                        style: chip(lv.consistency == percent),
                        title: "100%: every stroke brought to one level",
                        onclick: move |_| leveling.with_mut(|l| l.consistency = percent),
                        "{percent}%"
                    }
                }
            }
            div {
                style: ROW,
                for db in [6_u32, 12, 24] {
                    button {
                        key: "{db}",
                        style: chip(lv.range_db == db),
                        title: "The most a stroke is raised or lowered",
                        onclick: move |_| leveling.with_mut(|l| l.range_db = db),
                        "±{db} dB"
                    }
                }
                button {
                    style: chip(!lv.whole_song),
                    title: "Each stroke toward the strokes around it: a quiet verse stays quieter",
                    onclick: move |_| leveling.with_mut(|l| l.whole_song = false),
                    "Around it"
                }
                button {
                    style: chip(lv.whole_song),
                    title: "Every stroke toward the drum's level over the whole song",
                    onclick: move |_| leveling.with_mut(|l| l.whole_song = true),
                    "Whole song"
                }
                button {
                    style: chip(true),
                    disabled: busy(),
                    onclick: move |_| {
                        let args = level_args(&leveling.read());
                        busy.set(true);
                        said.set(vec!["Leveling…".into()]);
                        spawn(async move {
                            said.set(match run("DRUMS_LEVEL", args).await {
                                Ok(answer) => level_lines(&answer),
                                Err(why) => vec![why],
                            });
                            busy.set(false);
                        });
                    },
                    "Level"
                }
            }
            if !said.read().is_empty() {
                div {
                    style: "display:flex; flex-direction:column; gap:2px; color:#c9ccd1; white-space:pre-wrap;",
                    for line in said.read().iter().cloned() {
                        div { "{line}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_section_is_quantized_with_its_settings() {
        let choices = Choices {
            section: Some("BD".into()),
            grid: 16,
            strength: 90,
            ..Choices::default()
        };
        let args: Value =
            serde_json::from_str(&quantize_args(&choices, true).expect("args")).expect("json");
        assert_eq!(args["sections"][0], "BD");
        assert_eq!(args["grid"], 16);
        assert_eq!(args["target"], "Pattern");
        assert_eq!(args["dry_run"], true);
        assert!((args["strength"].as_f64().unwrap() - 0.9).abs() < 1e-9);
        assert!(args.get("lock").is_none());
    }

    #[test]
    fn a_locked_kick_or_a_selection_says_so() {
        let choices = Choices {
            selection: Some((10.0, 20.0)),
            lock_kick: true,
            ..Choices::default()
        };
        let args: Value =
            serde_json::from_str(&quantize_args(&choices, false).expect("args")).expect("json");
        assert_eq!(args["from"], 10.0);
        assert_eq!(args["lock"], "Kick");
        assert!(args.get("target").is_none(), "the lock places the kick");
        assert!(
            quantize_args(&Choices::default(), true).is_none(),
            "nowhere to quantize"
        );
    }

    #[test]
    fn q_quantizes_as_apply_last_did() {
        let applied = Choices {
            section: Some("VS 1".into()),
            grid: 8,
            target: "Grid",
            strength: 75,
            fills: "Include",
            ..Choices::default()
        };
        let args: Value =
            serde_json::from_str(&again_over(Some(&applied), (4.0, 9.5))).expect("json");
        assert!(args.get("sections").is_none(), "the span, not the section");
        assert_eq!(args["from"], 4.0);
        assert_eq!(args["to"], 9.5);
        assert_eq!(args["grid"], 8);
        assert_eq!(args["target"], "Grid");
        assert_eq!(args["fills"], "Include");
        assert_eq!(args["dry_run"], false);
        let locked = Choices {
            lock_kick: true,
            ..applied
        };
        let args: Value =
            serde_json::from_str(&again_over(Some(&locked), (4.0, 9.5))).expect("json");
        assert_eq!(args["lock"], "Kick");

        let fresh: Value = serde_json::from_str(&again_over(None, (1.0, 2.0))).expect("json");
        assert!(
            fresh.get("grid").is_none() && fresh.get("target").is_none(),
            "no Apply yet: the sticky settings decide"
        );
        assert_eq!(fresh["to"], 2.0);
    }

    #[test]
    fn apply_keeps_its_settings_sticky_and_the_panel_reads_them_back() {
        let applied = Choices {
            grid: 24,
            target: "Guided",
            strength: 90,
            fills: "Only",
            lock_kick: true,
            ..Choices::default()
        };
        let sticky: Value = serde_json::from_str(&sticky_args(&applied)).expect("json");
        assert_eq!(sticky["grid"], 24);
        assert_eq!(sticky["target"], "Guided");
        assert!(sticky.get("lock").is_none(), "a lock is not a setting");
        let back = from_sticky(Choices::default(), &sticky);
        assert_eq!(
            back,
            Choices {
                lock_kick: false,
                ..applied
            }
        );
        let other = serde_json::json!({"target": "Reference", "grid": 16});
        assert_eq!(
            from_sticky(Choices::default(), &other).target,
            "Pattern",
            "a target the panel does not offer leaves its own"
        );
    }

    #[test]
    fn put_back_names_its_stretch() {
        let section = Choices {
            section: Some("BD".into()),
            ..Choices::default()
        };
        let args: Value =
            serde_json::from_str(&revert_args(&section).expect("args")).expect("json");
        assert_eq!(args["sections"][0], "BD");
        let selection = Choices {
            selection: Some((10.0, 12.5)),
            ..Choices::default()
        };
        let args: Value =
            serde_json::from_str(&revert_args(&selection).expect("args")).expect("json");
        assert_eq!(args["to"], 12.5);
        assert!(revert_args(&Choices::default()).is_none());
    }

    #[test]
    fn the_level_knob_reaches_the_action() {
        let all = Leveling {
            consistency: 100,
            range_db: 24,
            whole_song: true,
        };
        let args: Value = serde_json::from_str(&level_args(&all)).expect("json");
        assert_eq!(args["consistency"], 100);
        assert_eq!(args["range_db"], 24);
        assert_eq!(args["whole_song"], true);
        let answer = serde_json::json!([{"piece": "Kick", "strokes": 1000, "raised": 477,
            "lowered": 416, "most_db": 6.0}]);
        assert_eq!(
            level_lines(&answer),
            ["Kick: 1000 strokes, 477 up, 416 down, at most +6.0 dB"]
        );
    }

    #[test]
    fn an_answer_reads_as_lines() {
        let answer = serde_json::json!([{
            "name": "BD",
            "result": {"moved": 60, "in_place": 2, "unmatched": 1, "in_fills": 8},
            "by_groove": [{}, {}],
            "check": {"ok": false, "lost": 0, "doubled": 1, "off_target": 0, "strayed": 0,
                      "problems": ["k plays twice"]}
        }]);
        let lines = summary(&answer);
        assert!(lines[0].contains("60 move") && lines[0].contains("2 placed by the groove"));
        assert!(lines[1].starts_with("check failed: 0 lost, 1 doubled"));
        assert_eq!(lines[2], "  k plays twice");
        let groove = serde_json::json!([{
            "cycle_bars": 2, "placed": 0.87,
            "pieces": [{"piece": "Snare", "slots": [{"at": "1:3"}, {"at": "2:3"}]}]
        }]);
        assert_eq!(
            groove_lines(&groove),
            [
                "two-bar cycle, 87% of its hits in the pattern",
                "Snare: 1:3 2:3"
            ]
        );
        let none = serde_json::json!([{"cycle_bars": 1, "placed": 0.0, "pieces": []}]);
        assert_eq!(
            groove_lines(&none),
            ["no pattern: no piece plays the same in most bars"]
        );
    }
}
