//! The rig's blocks drawn with frame faces.
//!
//! Signal decides what is loaded: presets, modules and overrides put a
//! pedal preset in each drive slot and a NAM capture in each amp. frame's
//! `rig-faces/faces.json` only says what those look like — a drive slot's
//! pedal preset name, or an amp's capture file name, matched against each
//! entry's words (case-insensitive, first entry wins; `a+b` needs every
//! word). Nothing matched, the
//! block is a plain card with its name.
//!
//! A face binds to its block by convention: `<ns>/<param>` is the block's
//! param (`drive`, `threshold`, `b3_freq`…), `<ns>/level` its output level
//! (dB), `<ns>/on` its bypass (on = not bypassed), `<ns>/option` its
//! capture choice. `<ns>/tone` is the drive block's EQ, still to come.

use dioxus::prelude::*;
use signal_guitar_proto::LiveBlock;
use signal_guitar_proto::rig::RigClient;
use signal_proto::BlockType;

use crate::frame_surface::FrameSurface;
use crate::param_writer::WriteParam;

/// One face in the manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceEntry {
    /// Its path under the faces directory, without `.fm`.
    pub face: String,
    /// The namespace its params are under.
    pub ns: String,
    /// Words that pick it (lower case).
    pub words: Vec<String>,
    /// Its size, for its box's proportion.
    pub size: (f64, f64),
    /// Other blocks the face also draws: (namespace, block name) — the
    /// Gravity Tank's `trem/` controls are the Pre Motion's.
    pub also: Vec<(String, String)>,
    /// The face laid out for other proportions: (face, width ÷ height). A
    /// face with variants fills its panel, drawing whichever fits.
    pub variants: Vec<(String, f64)>,
    /// Face params that aren't the block's own, and the block param each
    /// drives.
    pub maps: Vec<ParamMap>,
    /// The same face in its dark finish, where it has one: a Time lane
    /// underneath another draws this, so the pair reads light over dark.
    pub dark: Option<String>,
    /// The same design laid out for other rooms (`tiers` in the manifest):
    /// (tier, face, dark face, size). [`at`](Self::at) picks one.
    pub tiers: Vec<(crate::control::Tier, FaceTier)>,
    /// It carries the nameplate: prints its algorithm and preset itself,
    /// the host filling `<ns>/preset` and answering its presses.
    pub nameplate: bool,
    /// It carries the Time knob (`<ns>/time_knob`, `/sync`, `/time_text`),
    /// the host mapping it onto the block (`time_face`).
    pub time_knob: bool,
    /// Its back — the advanced controls — and the back's dark finish: the
    /// face's flip arrow (`<ns>/flip`) turns it over.
    pub back: Option<String>,
    pub back_dark: Option<String>,
}

/// A face's version for one tier — and its proportions there (`variants`:
/// a phone's amp, one, two or three to a row): (face, dark face, size, back, dark back).
#[derive(Debug, Clone, PartialEq)]
pub struct FaceTier {
    pub face: String,
    pub dark: Option<String>,
    pub size: (f64, f64),
    pub back: Option<String>,
    pub back_dark: Option<String>,
    pub variants: Vec<(String, f64)>,
    /// It carries the Time knob where the base face may not (a phone's
    /// modulation lane).
    pub time_knob: Option<bool>,
    /// …and the nameplate.
    pub nameplate: Option<bool>,
}

impl FaceEntry {
    /// This face laid out for `tier`: its version for that room, or the
    /// nearest smaller one it has (a phone's in a strip's room), else
    /// itself.
    #[must_use]
    pub fn at(&self, tier: crate::control::Tier) -> Self {
        let best = self.tiers.iter().filter(|(t, _)| *t >= tier && *t < crate::control::Tier::Desktop).min_by_key(|(t, _)| *t);
        let best = best.or_else(|| self.tiers.iter().filter(|(t, _)| *t <= tier).max_by_key(|(t, _)| *t));
        let mut f = self.clone();
        if let Some((_, t)) = best {
            f.face.clone_from(&t.face);
            f.dark.clone_from(&t.dark);
            f.size = t.size;
            f.back.clone_from(&t.back);
            f.back_dark.clone_from(&t.back_dark);
            f.variants.clone_from(&t.variants);
            if let Some(k) = t.time_knob {
                f.time_knob = k;
            }
            if let Some(n) = t.nameplate {
                f.nameplate = n;
            }
        }
        f
    }

    /// This face in its dark finish (itself when it has none).
    #[must_use]
    pub fn darkened(&self) -> Self {
        let mut f = self.clone();
        if let Some(d) = &self.dark {
            f.face.clone_from(d);
        }
        if self.back_dark.is_some() {
            f.back.clone_from(&self.back_dark);
        }
        f
    }
}

/// A face param (`<ns>/<face>`) standing for block param `param`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamMap {
    pub face: String,
    pub param: String,
    pub kind: MapKind,
}

/// How a face param's value becomes its block param's.
#[derive(Debug, Clone, PartialEq)]
pub enum MapKind {
    /// block = face × a + b.
    Linear(f64, f64),
    /// block = table[face], the face's value an option index.
    Table(Vec<f64>),
}

impl ParamMap {
    fn to_block(&self, face: f64) -> f64 {
        match &self.kind {
            MapKind::Linear(a, b) => face * a + b,
            MapKind::Table(t) => t.get(face.round().max(0.0) as usize).or(t.last()).copied().unwrap_or(face),
        }
    }

    fn to_face(&self, block: f64) -> f64 {
        match &self.kind {
            MapKind::Linear(a, b) if a.abs() > 1e-12 => (block - b) / a,
            MapKind::Linear(..) => 0.0,
            // The first option nearest the block's value.
            MapKind::Table(t) => t
                .iter()
                .enumerate()
                .min_by(|(_, x), (_, y)| (*x - block).abs().total_cmp(&(*y - block).abs()))
                .map_or(0.0, |(i, _)| i as f64),
        }
    }
}

/// `faces.json`, read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Faces {
    pub drives: Vec<FaceEntry>,
    pub amps: Vec<FaceEntry>,
    pub gate: Option<FaceEntry>,
    pub eq: Option<FaceEntry>,
    /// A cab beside its amp (wide screens).
    pub cab: Option<FaceEntry>,
    /// Pedals for the pre effects, by block preset name.
    pub pre: Vec<FaceEntry>,
    /// Pictures of what a block is doing, by its kind (`reverb`).
    pub pictures: Vec<FaceEntry>,
    /// The input stage's units (wah, filter), by block type.
    pub input: Vec<FaceEntry>,
    /// The filter's picture.
    pub filter_picture: Option<FaceEntry>,
    /// The post compressor's unit.
    pub post_comp: Option<FaceEntry>,
    /// The Time module's units: a face per delay machine, by its name.
    pub time_delay: Vec<FaceEntry>,
    /// … and per reverb algorithm.
    pub time_reverb: Vec<FaceEntry>,
    /// The Modulation and Motion modules' effects: a face per block type
    /// (chorus, tremolo, vibrato).
    pub modulation: Vec<FaceEntry>,
    /// The compressor pedal at the head of the drive board (the Pre Comp
    /// block).
    pub pre_comp_pedal: Option<FaceEntry>,
    /// The patch's level and pan (Patch Trim).
    pub trim: Option<FaceEntry>,
}

const SET: &str = "rig-faces";

fn entry(v: &serde_json::Value) -> Option<FaceEntry> {
    let face = v.get("face")?.as_str()?;
    let ns = v.get("ns")?.as_str()?.to_string();
    let words = v
        .get("match")
        .and_then(|m| m.as_array())
        .map(|a| a.iter().filter_map(|w| w.as_str().map(str::to_lowercase)).collect())
        .unwrap_or_default();
    let size = v
        .get("size")
        .and_then(|s| s.as_array())
        .and_then(|s| Some((s.first()?.as_f64()?, s.get(1)?.as_f64()?)))
        .unwrap_or((1.0, 1.0));
    let also = v
        .get("also")
        .and_then(|a| a.as_object())
        .map(|o| o.iter().filter_map(|(k, b)| Some((k.clone(), b.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let variants = v
        .get("variants")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let face = x.get("face")?.as_str()?;
                    let size = x.get("size")?.as_array()?;
                    let (w, h) = (size.first()?.as_f64()?, size.get(1)?.as_f64()?);
                    (h > 0.0).then(|| (format!("{SET}/{face}"), w / h))
                })
                .collect()
        })
        .unwrap_or_default();
    let maps = v
        .get("maps")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let face = m.get("face")?.as_str()?.to_string();
                    let param = m.get("param")?.as_str()?.to_string();
                    let kind = if let Some(l) = m.get("linear").and_then(|l| l.as_array()) {
                        MapKind::Linear(l.first()?.as_f64()?, l.get(1)?.as_f64()?)
                    } else {
                        MapKind::Table(m.get("table")?.as_array()?.iter().filter_map(serde_json::Value::as_f64).collect())
                    };
                    Some(ParamMap { face, param, kind })
                })
                .collect()
        })
        .unwrap_or_default();
    let dark = v.get("dark").and_then(|d| d.as_str()).map(|d| format!("{SET}/{d}"));
    let tiers = v
        .get("tiers")
        .and_then(|t| t.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(name, t)| {
                    let tier = crate::control::Tier::from_name(name)?;
                    let face = format!("{SET}/{}", t.get("face")?.as_str()?);
                    let named = |k: &str| t.get(k).and_then(|d| d.as_str()).map(|d| format!("{SET}/{d}"));
                    let s = t.get("size")?.as_array()?;
                    let variants = t
                        .get("variants")
                        .and_then(|a| a.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| {
                                    let face = x.get("face")?.as_str()?;
                                    let size = x.get("size")?.as_array()?;
                                    let (w, h) = (size.first()?.as_f64()?, size.get(1)?.as_f64()?);
                                    (h > 0.0).then(|| (format!("{SET}/{face}"), w / h))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((tier, FaceTier { face, dark: named("dark"), size: (s.first()?.as_f64()?, s.get(1)?.as_f64()?), back: named("back"), back_dark: named("back_dark"), variants, time_knob: t.get("time_knob").and_then(serde_json::Value::as_bool), nameplate: t.get("nameplate").and_then(serde_json::Value::as_bool) }))
                })
                .collect()
        })
        .unwrap_or_default();
    let nameplate = v.get("nameplate").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let time_knob = v.get("time_knob").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let back = v.get("back").and_then(|d| d.as_str()).map(|d| format!("{SET}/{d}"));
    let back_dark = v.get("back_dark").and_then(|d| d.as_str()).map(|d| format!("{SET}/{d}"));
    Some(FaceEntry { face: format!("{SET}/{face}"), ns, words, size, also, variants, maps, dark, tiers, nameplate, time_knob, back, back_dark })
}

impl Faces {
    /// Read the manifest; none (or a bad one) is an empty set — every
    /// block a card.
    #[must_use]
    pub fn load() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = crate::frame_surface::design_dir().join(SET).join("faces.json");
            let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
            let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
                tracing::warn!(target: "frame", manifest = %path.display(), "faces.json does not parse");
                return Self::default();
            };
            let list = |k: &str| doc.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(entry).collect()).unwrap_or_default();
            Self { drives: list("drives"), amps: list("amps"), gate: doc.get("gate").and_then(entry), eq: doc.get("eq").and_then(entry), cab: doc.get("cab").and_then(entry), pre: list("pre"), pictures: list("pictures"), input: list("input"), filter_picture: doc.get("filter_picture").and_then(entry), post_comp: doc.get("post_comp").and_then(entry), time_delay: doc.get("time").and_then(|t| t.get("delay")).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(entry).collect()).unwrap_or_default(), time_reverb: doc.get("time").and_then(|t| t.get("reverb")).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(entry).collect()).unwrap_or_default(), modulation: list("modulation"), pre_comp_pedal: doc.get("pre_comp_pedal").and_then(entry), trim: doc.get("trim").and_then(entry) }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::default()
        }
    }

    /// The face for a drive slot's pedal preset.
    #[must_use]
    pub fn drive(&self, preset: &str) -> Option<&FaceEntry> {
        pick(&self.drives, preset)
    }

    /// The face for a Modulation or Motion effect, by its block type's
    /// name (`chorus`, `trem`, `vibrato`).
    #[must_use]
    pub fn modulation(&self, block_type: &str) -> Option<&FaceEntry> {
        let name = block_type.to_lowercase();
        self.modulation.iter().find(|e| e.words.iter().any(|w| *w == name))
    }

    /// The face for a delay machine (`style` name) or reverb algorithm.
    #[must_use]
    pub fn time(&self, reverb: bool, algorithm: &str) -> Option<&FaceEntry> {
        let list = if reverb { &self.time_reverb } else { &self.time_delay };
        let name = algorithm.to_lowercase();
        list.iter().find(|e| e.words.iter().any(|w| *w == name))
    }

    /// The face for a pre effect's block preset.
    #[must_use]
    pub fn pre(&self, preset: &str) -> Option<&FaceEntry> {
        pick(&self.pre, preset)
    }

    /// The picture for a kind of block (`reverb`).
    #[must_use]
    pub fn picture(&self, kind: &str) -> Option<&FaceEntry> {
        pick(&self.pictures, kind)
    }

    /// The face for an amp's capture file.
    #[must_use]
    pub fn amp(&self, asset: &str) -> Option<&FaceEntry> {
        let file = asset.rsplit(['/', '\\']).next().unwrap_or(asset);
        pick(&self.amps, file)
    }
}

fn pick<'a>(list: &'a [FaceEntry], name: &str) -> Option<&'a FaceEntry> {
    let name = name.to_lowercase();
    if name.is_empty() {
        return None;
    }
    // `a+b` needs every word; otherwise any one.
    list.iter().find(|e| e.words.iter().any(|w| w.split('+').all(|part| name.contains(part.trim()))))
}

/// The manifest, read once per mount (a regenerated set is picked up by
/// reopening the view).
pub fn use_faces() -> Faces {
    use_hook(Faces::load)
}

/// A block's values as its face's params, in their own units.
fn values_of(block: &LiveBlock, ns: &str) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = block.params.iter().map(|p| (format!("{ns}/{}", p.name), f64::from(p.value))).collect();
    if let Some(db) = block.output_level_db {
        v.push((format!("{ns}/level"), f64::from(db)));
    }
    v.push((format!("{ns}/on"), if block.bypassed { 0.0 } else { 1.0 }));
    v.push((format!("{ns}/option"), f64::from(block.option)));
    v
}

/// An edit on a face, sent to its block.
fn send(rig: Option<RigClient>, block: &LiveBlock, ns: &str, addr: &str, value: f64) {
    let Some(rig) = rig else { return };
    let Some(name) = addr.strip_prefix(ns).and_then(|s| s.strip_prefix('/')) else { return };
    let id = block.id.clone();
    match name {
        "on" => {
            spawn(async move {
                let _ = rig.set_block_bypass(id, value < 0.5).await;
            });
        }
        // A param of the block's own first: a delay's `level` is its
        // wet's level, not the block's output.
        param if block.params.iter().any(|p| p.name == param) => {
            let _ = rig.write_param(id, param.to_string(), value as f32);
        }
        "level" => {
            spawn(async move {
                let _ = rig.set_block_level(id, value as f32, true).await;
            });
        }
        "option" => {
            let option = value.round().max(0.0) as u32;
            spawn(async move {
                let _ = rig.set_block_option(id, option).await;
            });
        }
        // The drive block's own EQ is still to come.
        _ => {}
    }
}

/// A block drawn by its face: the box takes the face's proportion from
/// the row's height.
///
/// `extra` are the other blocks it draws (its `also`), by namespace.
#[component]
pub fn BlockFace(
    block: LiveBlock,
    face: FaceEntry,
    #[props(default)] extra: Vec<(String, LiveBlock)>,
    /// Take the box's width (its height from the face's proportion)
    /// instead of its height.
    #[props(default)]
    across: bool,
    /// Fill the box exactly, edge to edge: the face laid out for the box's
    /// proportions (frame's responsive mode — its picture widens or
    /// narrows, its controls keep their shape), so no strip of the host
    /// shows beside it and nothing is squashed.
    #[props(default)]
    fill: bool,
    /// Live values the face shows that aren't params (a meter's level):
    /// (address, value).
    #[props(default)]
    live: Vec<(String, f64)>,
    /// Show the block's preset in the face's top-right corner (‹ name ›,
    /// the name opening its presets in the browser).
    #[props(default)]
    stepper: bool,
    /// For a face with the nameplate: the block's preset type as the
    /// libraries file it (`reverb`), and the param its algorithm menu sets
    /// with the options (`algorithm`, the reverb algorithms).
    #[props(default)]
    preset_type: Option<String>,
    #[props(default)]
    algos: Option<(&'static str, Vec<&'static str>)>,
    /// The tempo, for a face with the Time knob (its Beat side).
    #[props(default)]
    tempo_bpm: Option<u32>,
    /// The blocks it is one of (a modulation group: name, id), the one
    /// playing first among them by `member` — its algorithm menu picks
    /// among them too.
    #[props(default)]
    members: Vec<(String, String)>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    // The nameplate: the preset playing, printed by the face.
    let comp = try_use_context::<crate::face_chrome::FacePresets>().map(|c| c.0.read().clone()).unwrap_or_default();
    let playing = comp.active_blocks.iter().find(|b| b.block.eq_ignore_ascii_case(&block.name)).map(|b| b.preset.clone()).unwrap_or_default();
    let mut texts = if face.nameplate { vec![(format!("{}/preset", face.ns), if playing.is_empty() { "—".to_string() } else { playing.clone() })] } else { Vec::new() };
    // The Time knob: its position, switch and readout, from the block.
    let bpm = tempo_bpm.unwrap_or(120) as f32;
    let timed = face.time_knob.then(|| crate::time_face::view(&block, bpm)).flatten();
    // Turned over to its back (the advanced controls) by its flip arrow.
    // `FTS_FACE_BACK=1` starts every face turned over (to look at backs).
    let mut flipped = use_signal(|| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::env::var("FTS_FACE_BACK").is_ok_and(|v| v == "1")
        }
        #[cfg(target_arch = "wasm32")]
        false
    });
    let shown = match (&face.back, flipped()) {
        (Some(back), true) => back.clone(),
        _ => face.face.clone(),
    };
    // Where the last press on the face landed (window coordinates): the
    // algorithm menu opens there.
    let mut pressed_at = use_signal(|| (0.0, 0.0));
    let nameplate = face.nameplate.then(|| (block.clone(), preset_type.clone().unwrap_or_default(), algos.clone(), comp.clone(), playing.clone(), members.clone()));
    let mut values = values_of(&block, &face.ns);
    values.extend(live);
    for m in &face.maps {
        if let Some(p) = block.params.iter().find(|p| p.name == m.param) {
            values.push((format!("{}/{}", face.ns, m.face), m.to_face(f64::from(p.value))));
        }
    }
    if let Some(t) = &timed {
        // The pictures animate from the time as it plays.
        if let Some(ms) = t.playing_ms {
            values.retain(|(a, _)| *a != format!("{}/time", face.ns));
            values.push((format!("{}/time", face.ns), ms));
        }
        values.push((format!("{}/time_knob", face.ns), t.time));
        values.push((format!("{}/sync", face.ns), t.sync));
        texts.push((format!("{}/time_text", face.ns), t.text.clone()));
    }
    let time_block = timed.is_some().then(|| block.clone());
    let maps = face.maps.clone();
    for (ns, b) in &extra {
        values.extend(values_of(b, ns));
    }
    let (w, h) = face.size;
    let mut targets: Vec<(String, LiveBlock)> = vec![(face.ns.clone(), block.clone())];
    targets.extend(extra);
    // A face with variants fills its panel (it reflows to the box); one
    // without keeps its own proportion.
    let boxed = if fill {
        "width: 100%; height: 100%; min-width: 0; min-height: 0;".to_string()
    } else if across {
        format!("width: 100%; aspect-ratio: {w} / {h}; flex: 0 0 auto; min-width: 0;")
    } else if face.variants.is_empty() {
        format!("height: 100%; aspect-ratio: {w} / {h}; flex: 0 0 auto; min-height: 0;")
    } else {
        "height: 100%; flex: 1 1 0%; min-width: 0; min-height: 0;".to_string()
    };
    let (step_name, step_type) = (block.name.clone(), block.block_type.as_str().to_string());
    rsx! {
        div { style: "position: relative; {boxed}",
            onpointerdown: move |e: PointerEvent| {
                let c = e.client_coordinates();
                pressed_at.set((c.x, c.y));
            },
            if stepper && !face.nameplate {
                PresetCorner { block: step_name, block_type: step_type }
            }
            FrameSurface {
                key: "{block.id}-{shown}",
                name: shown.clone(),
                responsive: fill,
                variants: face.variants.clone(),
                values,
                texts,
                on_edit: move |(addr, value): (String, f64)| {
                    // The flip arrow: the other side.
                    if addr.ends_with("/flip") {
                        if value >= 0.5 {
                            flipped.toggle();
                        }
                        return;
                    }
                    // The Time knob and its switch, onto the block.
                    if let Some(b) = time_block.as_ref()
                        && let Some((_, what)) = addr.rsplit_once('/').filter(|(_, w)| *w == "time_knob" || *w == "sync")
                    {
                        let writes = if what == "time_knob" {
                            crate::time_face::turn(b, bpm, value)
                        } else {
                            // A press on the side already lit changes nothing.
                            let beat = value >= 0.5;
                            let now = crate::time_face::view(b, bpm).is_some_and(|t| t.sync >= 0.5);
                            if beat == now { Vec::new() } else { crate::time_face::flip(b, bpm, beat) }
                        };
                        for (param, v) in writes {
                            crate::control::send_param(&rig, &b.id, param, v);
                        }
                        return;
                    }
                    // The nameplate's presses.
                    if let Some((b, kind, algos, comp, playing, members)) = nameplate.as_ref()
                        && let Some(press) = ["algo_menu", "preset_prev", "preset_next", "preset_browse"].into_iter().find(|p| addr.ends_with(&format!("/{p}")))
                    {
                        if value >= 0.5 {
                            nameplate_press(rig.clone(), press, b, kind, algos.as_ref(), comp, playing, pressed_at(), members);
                        }
                        return;
                    }
                    // A mapped face param drives its block param.
                    if let Some((ns, b)) = targets.first()
                        && let Some(m) = maps.iter().find(|m| addr == format!("{ns}/{}", m.face))
                    {
                        if let Some(r) = rig.clone() {
                            let _ = r.write_param(b.id.clone(), m.param.clone(), m.to_block(value) as f32);
                        }
                        return;
                    }
                    if let Some((ns, b)) = targets.iter().find(|(ns, _)| addr.starts_with(&format!("{ns}/"))) {
                        send(rig.clone(), b, ns, &addr, value);
                    }
                },
            }
        }
    }
}

/// A press on a face's nameplate: step the block's preset (‹ ›), open its
/// presets (the name), or open its algorithm menu at `at` (the title).
#[allow(clippy::too_many_arguments)]
fn nameplate_press(
    rig: Option<RigClient>,
    press: &str,
    block: &LiveBlock,
    kind: &str,
    algos: Option<&(&'static str, Vec<&'static str>)>,
    comp: &signal_guitar_proto::CompositionModel,
    playing: &str,
    at: (f64, f64),
    members: &[(String, String)],
) {
    match press {
        "preset_prev" | "preset_next" => {
            let names: Vec<String> = comp.block_presets.iter().filter(|p| p.block_type.eq_ignore_ascii_case(kind)).map(|p| p.name.clone()).collect();
            if names.is_empty() {
                return;
            }
            let n = names.len() as i64;
            let dir = if press == "preset_next" { 1 } else { -1 };
            let next = names.iter().position(|x| x.eq_ignore_ascii_case(playing)).map_or(0, |i| (i as i64 + dir).rem_euclid(n) as usize);
            let (block, name) = (block.name.clone(), names[next].clone());
            spawn(async move {
                if let Some(r) = rig {
                    let _ = r.choose_block(block, name).await;
                }
            });
        }
        "preset_browse" => {
            if let Some(s) = try_consume_context::<crate::module_sidebar::SelectedModule>() {
                s.set(crate::module_sidebar::Selection::Block { name: block.name.clone(), block_type: kind.to_string() });
            }
        }
        "algo_menu" => {
            // A face picked by its preset (a Pre FX pedal): its name opens
            // the presets. On a phone, every pick opens the browser on the
            // block (its algorithm and its presets) — no popup grid.
            if algos.is_none() || try_consume_context::<crate::phone::PhoneBrowses>().is_some() {
                if let Some(s) = try_consume_context::<crate::module_sidebar::SelectedModule>() {
                    s.set(crate::module_sidebar::Selection::Block { name: block.name.clone(), block_type: kind.to_string() });
                }
                return;
            }
            let (Some((param, options)), Some(host)) = (algos, signal_widgets::PopupHost::try_use()) else { return };
            let (param, options) = (*param, options.clone());
            let current = block.params.iter().find(|p| p.name == param).map_or(0, |p| p.value.round().max(0.0) as usize);
            let id = block.id.clone();
            let members = members.to_vec();
            let playing_id = block.id.clone();
            host.open(
                at.0,
                at.1 + 12.0,
                260.0,
                move || {
                    let engines = crate::control::algo_grid(&options, current, "#c4b5fd", {
                        let (rig, id) = (rig.clone(), id.clone());
                        move |i| {
                            crate::control::send_param(&rig, &id, param, i as f32);
                            spawn(async move { host.close() });
                        }
                    });
                    if members.len() < 2 {
                        return engines;
                    }
                    // A group's members first: a press plays that one (the
                    // others bypassed); its engines under them.
                    let (rig, members2) = (rig.clone(), members.clone());
                    rsx! {
                        div { style: "display: flex; flex-direction: column; gap: 4px;",
                            div { style: "display: flex; gap: 4px; padding: 8px 8px 0 8px; background: #18181b; border-radius: 8px 8px 0 0;",
                                for (name, mid) in members.iter().cloned() {
                                    {
                                        let on = mid == playing_id;
                                        let (rig, all) = (rig.clone(), members2.clone());
                                        rsx! {
                                            button { key: "{mid}",
                                                style: if on { "flex: 1 1 0%; padding: 8px 4px; border-radius: 6px; font-size: 12px; font-weight: 700; background: #e4e4e7; color: #09090b;" } else { "flex: 1 1 0%; padding: 8px 4px; border-radius: 6px; font-size: 12px; font-weight: 600; background: #27272a; color: #d4d4d8;" },
                                                onclick: move |_| {
                                                    let (rig, all, mid) = (rig.clone(), all.clone(), mid.clone());
                                                    spawn(async move {
                                                        if let Some(r) = rig {
                                                            for (_, other) in &all {
                                                                let _ = r.set_block_bypass(other.clone(), *other != mid).await;
                                                            }
                                                        }
                                                        host.close();
                                                    });
                                                },
                                                "{name}"
                                            }
                                        }
                                    }
                                }
                            }
                            {engines}
                        }
                    }
                },
                || {},
            );
        }
        _ => {}
    }
}

/// A block with no face: a card with what it plays, bypass on a click.
/// `aspect` is the box it stands in for (a pedal's, an amp's).
#[component]
pub fn NameCard(block: LiveBlock, aspect: (f64, f64)) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let (w, h) = aspect;
    let title = if block.empty { "Empty".to_string() } else if block.preset.is_empty() { block.name.clone() } else { block.preset.clone() };
    let detail = if !block.detail.is_empty() {
        block.detail.clone()
    } else {
        block.asset.rsplit(['/', '\\']).next().unwrap_or_default().trim_end_matches(".nam").to_string()
    };
    let on = !block.bypassed && !block.empty;
    let id = block.id.clone();
    let empty = block.empty;
    let opacity = if empty { 0.45 } else { 1.0 };
    let lamp = if on { "#4ade80" } else { "#3a3d44" };
    rsx! {
        div {
            style: "height: 100%; aspect-ratio: {w} / {h}; flex: 0 0 auto; min-height: 0; display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 6px; padding: 10px; box-sizing: border-box; opacity: {opacity};",
            onclick: move |_| {
                if empty {
                    return;
                }
                let (rig, id) = (rig.clone(), id.clone());
                spawn(async move {
                    if let Some(r) = rig {
                        let _ = r.toggle_block_bypass(id).await;
                    }
                });
            },
            div { style: "width: 8px; height: 8px; border-radius: 4px; background: {lamp};" }
            div { style: "color: #e8e6e1; font-size: 13px; font-weight: 600; text-align: center; overflow: hidden;", "{title}" }
            if !detail.is_empty() {
                div { style: "color: #8a8d94; font-size: 10px; text-align: center; overflow: hidden;", "{detail}" }
            }
            div { style: "color: #5c5f66; font-size: 9px; letter-spacing: 1px; text-transform: uppercase;", "{block.name}" }
        }
    }
}

/// The height-to-width a pedal card stands in (a Standard pedal).
const PEDAL: (f64, f64) = (180.0, 286.0);
/// The height-to-width an amp card stands in.
const AMP: (f64, f64) = (792.0, 392.0);

/// A panel in a row: its own ground, flush against its neighbours (no
/// gaps, no padding). Its faces come first — each takes its width from
/// the row's full height — and the panel grows by `weight` into what the
/// faces leave (a visualiser fills it, or the panel's ground does). A
/// panel of weight 0 is exactly its faces.
#[component]
fn Panel(weight: f64, children: Element) -> Element {
    let flex = if weight > 0.0 { format!("{weight} 1 auto") } else { "0 0 auto".to_string() };
    rsx! {
        div {
            style: "flex: {flex}; height: 100%; min-height: 0; display: flex; flex-direction: row; align-items: stretch; justify-content: center; background: #121317; border-right: 1px solid #1d1f24; box-sizing: border-box; overflow: hidden;",
            {children}
        }
    }
}

/// A row of panels, flush, the full width and height it is given.
#[component]
fn Row(children: Element) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: row; align-items: stretch; width: 100%; height: 100%; min-height: 0; gap: 0; padding: 0;",
            {children}
        }
    }
}

/// A block in its panel: its face, or its card.
#[component]
fn BlockPanel(
    block: LiveBlock,
    face: Option<FaceEntry>,
    card: (f64, f64),
    /// How much of the row's spare width it takes (0: exactly its face).
    #[props(default = 1.0)]
    weight: f64,
) -> Element {
    rsx! {
        Panel { weight,
            if let Some(f) = face {
                BlockFace { block, face: f }
            } else {
                NameCard { block, aspect: card }
            }
        }
    }
}

/// The Drives row, as a pedalboard: the drive board's slots in chain
/// order, each its pedal's face (or a card) standing on the board's rails,
/// the pedals joined by patch cables into their side jacks — the cables
/// take whatever width the pedals leave — a lead in from the guitar and
/// one out to the amps, and the daisy-chained power along the top.
#[component]
pub fn DrivesRow(
    blocks: Vec<LiveBlock>,
    /// Stood at the head of the board, before the pedals and cabled into
    /// the first: the compressor's visualiser.
    #[props(default)]
    leading: Option<Element>,
    /// Fit every pedal into this width (points) — the phone, where the
    /// full height would leave room for two — rather than standing each
    /// the row's full height.
    #[props(default)]
    fit_width: Option<f64>,
    /// …and no taller than this (points).
    #[props(default)]
    fit_height: Option<f64>,
) -> Element {
    let faces = use_faces();
    // Each pedal at the window's tier (on a phone, its narrow version).
    let tier = crate::control::use_tier();
    let mut slots: Vec<(LiveBlock, Option<FaceEntry>)> = Vec::new();
    slots.extend(
        blocks
            .iter()
            .filter(|b| matches!(b.block_type, BlockType::Boost | BlockType::Drive))
            .map(|b| (b.clone(), (!b.empty).then(|| faces.drive(&b.preset).map(|f| f.at(tier))).flatten())),
    );
    let last = slots.len().saturating_sub(1);
    // Fitted: the pedals' height that puts them all across `fit_width`,
    // after the cables' least room.
    let pedal_h = fit_width.map(|w| {
        let aspects: f64 = slots.iter().map(|(_, f)| f.as_ref().map_or(PEDAL.0 / PEDAL.1, |f| f.size.0 / f.size.1)).sum();
        // Fitted, the cables take their least: the pedals get the room.
        let cables = TIGHT_CABLE * last as f64 + 2.0 * TIGHT_CABLE;
        ((w - cables) / aspects.max(0.1)).max(60.0).min(fit_height.unwrap_or(f64::MAX))
    });
    let tight = fit_width.is_some();
    let slot_size = pedal_h.map_or_else(|| "height: 100%;".to_string(), |h| format!("height: {h:.0}px; align-self: center;"));
    rsx! {
        div { style: "position: relative; display: flex; flex-direction: row; align-items: stretch; width: 100%; height: 100%; min-height: 0; box-sizing: border-box; padding: {BOARD_PAD_TOP}px 0 {BOARD_PAD_BOTTOM}px 0; background: {BOARD};",
            // The power: one lead along the top, a drop into each pedal.
            div { style: "position: absolute; left: 0; right: 0; top: 4px; height: 3px; border-radius: 2px; background: {CORD}; pointer-events: none;" }
            Cable { end: CableEnd::In, tight }
            if let Some(lead) = leading {
                // Its own dark ground: a unit on the board, not a hole in it.
                div { style: "position: relative; height: 100%; min-height: 0; flex: 0 1 300px; min-width: 200px; display: flex; background: #0b0c0f; border-radius: 6px; overflow: hidden; box-shadow: 0 3px 8px rgba(0,0,0,0.6);",
                    {lead}
                }
                Cable { end: CableEnd::Between, tight }
            }
            for (i, (b, face)) in slots.into_iter().enumerate() {
                div { key: "{b.id}", style: "position: relative; {slot_size} min-height: 0; flex: 0 0 auto; display: flex;",
                    div { style: "position: absolute; left: 50%; top: -{BOARD_PAD_TOP}px; width: 3px; height: {BOARD_PAD_TOP + 4}px; margin-left: -1px; background: {CORD}; pointer-events: none;" }
                    if let Some(f) = face {
                        BlockFace { block: b.clone(), face: f }
                    } else {
                        NameCard { block: b.clone(), aspect: PEDAL }
                    }
                }
                Cable { end: if i == last { CableEnd::Out } else { CableEnd::Between }, tight }
            }
        }
    }
}

/// The board under the pedals: dark anodised rails across it, barely lit
/// along their top edges (the board stays behind the pedals; the black
/// cables still read against it), the gaps between them black. One gradient, hard
/// stops.
const BOARD: &str = "linear-gradient(180deg, #060607 0%, #060607 3%, #18191c 3%, #131416 4%, #101113 18%, #0d0e10 20%, #060607 20%, #060607 24%, #18191c 24%, #131416 25%, #101113 41%, #0d0e10 43%, #060607 43%, #060607 47%, #18191c 47%, #131416 48%, #101113 64%, #0d0e10 66%, #060607 66%, #060607 70%, #18191c 70%, #131416 71%, #101113 87%, #0d0e10 89%, #060607 89%, #060607 100%)";
/// A cable's black.
const CORD: &str = "#050506";
/// The board shows above and below the pedals (the power runs above).
const BOARD_PAD_TOP: u32 = 12;
const BOARD_PAD_BOTTOM: u32 = 8;
/// A cable's least width where the board fits its pedals to a width.
const TIGHT_CABLE: f64 = 12.0;
/// Where a pedal's side jacks sit, down its height (the faces' jacks).
const JACK_AT: f64 = 60.0;

#[derive(Clone, Copy, PartialEq)]
enum CableEnd {
    /// The lead in from the guitar, to the first pedal.
    In,
    /// A patch cable, pedal to pedal.
    Between,
    /// The lead out to the amps, from the last pedal.
    Out,
}

/// A cable in a gap of the board: a patch cable sagging from one pedal's
/// jack to the next's, right-angle plugs at both ends; or a lead running
/// off the board's edge.
#[component]
fn Cable(end: CableEnd, #[props(default)] tight: bool) -> Element {
    let flex = match (end == CableEnd::Between, tight) {
        (true, false) => "1 1 0%; min-width: 40px;".to_string(),
        (false, false) => "0 1 36px; min-width: 18px;".to_string(),
        (_, true) => format!("1 1 0%; min-width: {TIGHT_CABLE}px;"),
    };
    let plug = |side: &str| format!("position: absolute; {side}: 0; top: {JACK_AT}%; width: 12px; height: 12px; margin-top: -6px; border-radius: 2px; background: linear-gradient(180deg, #e9eaec 0%, #8e9196 55%, #4a4c50 100%); pointer-events: none;");
    let boot = |side: &str| format!("position: absolute; {side}: 10px; top: {JACK_AT}%; width: 10px; height: 16px; margin-top: -8px; border-radius: 3px; background: linear-gradient(90deg, #1b1b1d 0%, #2c2c30 50%, #151517 100%); pointer-events: none;");
    rsx! {
        div { style: "position: relative; flex: {flex} height: 100%; pointer-events: none;",
            match end {
                CableEnd::Between => rsx! {
                    div { style: "{plug(\"left\")}" }
                    div { style: "{boot(\"left\")}" }
                    div { style: "{plug(\"right\")}" }
                    div { style: "{boot(\"right\")}" }
                    // The cable, sagging between the boots.
                    div { style: "position: absolute; left: 14px; right: 14px; top: {JACK_AT}%; bottom: 4%; border: 5px solid {CORD}; border-top: none; border-radius: 0 0 26px 26px; pointer-events: none;" }
                },
                CableEnd::In => rsx! {
                    div { style: "{plug(\"right\")}" }
                    div { style: "{boot(\"right\")}" }
                    div { style: "position: absolute; left: 0; right: 18px; top: {JACK_AT}%; height: 5px; margin-top: -2px; background: {CORD}; pointer-events: none;" }
                },
                CableEnd::Out => rsx! {
                    div { style: "{plug(\"left\")}" }
                    div { style: "{boot(\"left\")}" }
                    div { style: "position: absolute; left: 18px; right: 0; top: {JACK_AT}%; height: 5px; margin-top: -2px; background: {CORD}; pointer-events: none;" }
                },
            }
        }
    }
}

/// The Amp row: every loaded amp, its face or a card; with one amp, the
/// Amp EQ's graph beside it; the gate's picture on the right.
#[component]
pub fn AmpRow(
    blocks: Vec<LiveBlock>,
    #[props(default)] cabs: bool,
    /// Drawn last, after the gate: the patch's level and pan.
    #[props(default = VNode::empty())]
    trailing: Element,
    /// The input's live peak (dBFS), for the gate's meter.
    #[props(default)]
    in_db: Option<Signal<f32>>,
    /// The amps alone: no EQ beside one, no gate (the phone has pages
    /// for those).
    #[props(default)]
    amps_only: bool,
    /// The box the amps must fit (the phone's page: the screen less the
    /// rail, the housing's clearance and the bars). At full height three
    /// amp heads are wider than a phone, and ran on under the rail and off
    /// the screen; given this, every amp shrinks by the same factor until
    /// the row fits, centred.
    #[props(default)]
    fit: Option<(f64, f64)>,
) -> Element {
    let faces = use_faces();
    // Each amp at the window's tier (upright on a phone).
    let tier = crate::control::use_tier();
    // Every amp the preset loaded: the first always, any other when it
    // plays something (a stereo module's Amp R).
    let amps: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.block_type == BlockType::Amp && !b.empty)
        .enumerate()
        .filter(|(i, b)| *i == 0 || !b.asset.is_empty())
        .map(|(_, b)| b.clone())
        .collect();
    let eq = blocks.iter().find(|b| b.block_type == BlockType::Eq && b.name.eq_ignore_ascii_case("Amp EQ")).cloned();
    let gate = blocks.iter().find(|b| b.block_type == BlockType::Gate).cloned().filter(|_| !amps_only);
    let one = amps.len() == 1 && !amps_only;
    // Fitted: each amp's box, in points — its face's proportion at the
    // page's height, scaled down together until the row is no wider than
    // the page.
    if let Some((fw, fh)) = fit {
        let face_of = |b: &LiveBlock| faces.amp(&b.asset).or_else(|| faces.amp(&b.preset)).map(|f| f.at(tier));
        let aspect = |b: &LiveBlock| face_of(b).map_or(AMP.0 / AMP.1, |f| f.size.0 / f.size.1.max(1.0));
        let total: f64 = amps.iter().map(|b| fh * aspect(b)).sum();
        let scale = if total > fw && total > 0.0 { fw / total } else { 1.0 };
        let h = (fh * scale).floor();
        return rsx! {
            div { style: "display: flex; flex-direction: row; align-items: center; justify-content: center; width: 100%; height: 100%; min-width: 0; min-height: 0; overflow: hidden;",
                for b in amps {
                    {
                        let w = (h * aspect(&b)).floor();
                        rsx! {
                            div { key: "{b.id}", style: "flex: 0 0 {w}px; width: {w}px; height: {h}px; display: flex; min-width: 0; min-height: 0;",
                                BlockPanel { face: face_of(&b), block: b.clone(), card: AMP }
                            }
                        }
                    }
                }
            }
        };
    }
    rsx! {
        Row {
            for b in amps {
                if let Some((cab, face)) = cabs.then(|| cab_of(&blocks, &b).zip(faces.cab.clone())).flatten() {
                    // The amp and its cab, halves of one panel.
                    Panel { key: "{b.id}", weight: 1.0,
                        if let Some(f) = faces.amp(&b.asset).or_else(|| faces.amp(&b.preset)).map(|f| f.at(tier)) {
                            BlockFace { block: b.clone(), face: f }
                        } else {
                            NameCard { block: b.clone(), aspect: AMP }
                        }
                        BlockFace { block: cab, face }
                    }
                } else {
                    BlockPanel { key: "{b.id}", face: faces.amp(&b.asset).or_else(|| faces.amp(&b.preset)).map(|f| f.at(tier)), block: b.clone(), card: AMP }
                }
            }
            if one {
                if let (Some(e), Some(f)) = (eq, faces.eq.clone()) {
                    BlockPanel { block: e, face: Some(f), card: AMP, weight: 0.0 }
                }
            }
            if let (Some(g), Some(f)) = (gate, faces.gate.clone()) {
                if let Some(level) = in_db {
                    Panel { weight: 0.0,
                        GateFace { block: g, face: f, level }
                    }
                } else {
                    BlockPanel { block: g, face: Some(f), card: (250.0, 392.0), weight: 0.0 }
                }
            }
            {trailing}
        }
    }
}

/// The gate's unit on its own, its meter live: a page of the phone's chain.
#[component]
pub fn GateUnit(blocks: Vec<LiveBlock>, in_db: Signal<f32>) -> Element {
    let faces = use_faces();
    let gate = blocks.iter().find(|b| b.block_type == BlockType::Gate).cloned();
    rsx! {
        if let (Some(g), Some(f)) = (gate, faces.gate.clone()) {
            Panel { weight: 1.0,
                GateFace { block: g, face: f, level: in_db }
            }
        }
    }
}

/// The gate's face with the input's level in its meter: its own
/// component, so only it redraws as the level moves.
#[component]
pub(crate) fn GateFace(block: LiveBlock, face: FaceEntry, level: Signal<f32>, #[props(default)] fill: bool) -> Element {
    let live = vec![(format!("{}/in", face.ns), f64::from(level()))];
    let preset_type = fill.then(|| "gate".to_string());
    rsx! { BlockFace { block, face, live, fill, preset_type } }
}

/// A pre effect whose face carries its own picture (the Aqua-Puss unit's
/// repeats screen): it fills its panel, with no visualiser beside it.
fn self_viewing(f: &FaceEntry) -> bool {
    f.face.ends_with("17-pedal-aqua-puss")
}

/// A block's preset, in a pill over a face's top-right corner.
#[component]
pub fn PresetCorner(block: String, block_type: String) -> Element {
    rsx! {
        div { style: "position: absolute; right: 6px; top: 6px; z-index: 5; display: flex; align-items: center; padding: 2px 4px; border-radius: 4px; background: rgba(8,8,8,0.72);",
            crate::face_chrome::PresetStepper { block, block_type, show_empty: true }
        }
    }
}

/// The pre effects row: the blocks in front of the amp, each its pedal's
/// face (by the block preset loaded) beside a picture of what it is doing
/// — the delay's repeats, the spring's tail — or a card. The delay first
/// (the John Mayer chain's slapback ahead of its tank).
#[component]
pub fn PreFxRow(
    blocks: Vec<LiveBlock>,
    tempo_bpm: u32,
    /// What of the row: everything, the units (modulation, tremolo), or
    /// the lanes (delay over reverb) — the phone shows them as two pages.
    #[props(default)]
    part: PrePart,
) -> Element {
    let faces = use_faces();
    let pre: Vec<LiveBlock> = blocks
        .iter()
        .filter(|b| b.name.starts_with("Pre ") && !b.name.eq_ignore_ascii_case("Pre Comp"))
        .cloned()
        .collect();
    // By its preset, else by the block itself (Pre Mod's unit for any).
    // Each at the window's tier.
    let tier = crate::control::use_tier();
    let picked: Vec<(LiveBlock, Option<FaceEntry>)> = pre.iter().map(|b| (b.clone(), faces.pre(&b.preset).or_else(|| faces.pre(&b.name)).map(|f| f.at(tier)))).collect();
    let absorbed: Vec<String> = picked
        .iter()
        .filter_map(|(_, f)| f.as_ref())
        .flat_map(|f| f.also.iter().map(|(_, name)| name.to_lowercase()))
        .collect();
    let mut items: Vec<(LiveBlock, Option<FaceEntry>, Vec<(String, LiveBlock)>)> = picked
        .into_iter()
        .filter(|(b, _)| !absorbed.contains(&b.name.to_lowercase()))
        .map(|(b, f)| {
            let extra = f
                .as_ref()
                .map(|f| {
                    f.also
                        .iter()
                        .filter_map(|(ns, name)| pre.iter().find(|x| x.name.eq_ignore_ascii_case(name)).map(|x| (ns.clone(), x.clone())))
                        .collect()
                })
                .unwrap_or_default();
            (b, f, extra)
        })
        .collect();
    items.sort_by_key(|(b, _, _)| b.block_type != BlockType::Delay);
    // The delay and the reverb as lanes, stacked — delay on top — in the
    // Time module's form and one lane's width, at the right; the rest (the
    // tremolo) before them — for the look, not the chain's order.
    let laned = |(b, f, _): &(LiveBlock, Option<FaceEntry>, Vec<(String, LiveBlock)>)| f.is_some() && matches!(b.block_type, BlockType::Delay | BlockType::Reverb);
    let lanes: Vec<(LiveBlock, FaceEntry, Vec<(String, LiveBlock)>)> = items.iter().filter(|i| laned(i)).filter_map(|(b, f, e)| f.clone().map(|f| (b.clone(), f, e.clone()))).collect();
    let items: Vec<(LiveBlock, Option<FaceEntry>, Vec<(String, LiveBlock)>)> = if part == PrePart::Lanes { Vec::new() } else { items.into_iter().filter(|i| !laned(i)).collect() };
    let lanes = if part == PrePart::Units { Vec::new() } else { lanes };
    // The lanes on their own page take the width; in the row, one Time
    // lane's.
    let lane_flex = if part == PrePart::Lanes { "1 1 0%" } else { "0 0 50.93%" };
    // Pre modulation takes what the tremolo and the lanes leave.
    let is_mod = |b: &LiveBlock| b.block_type == BlockType::Chorus;
    rsx! {
        Row {
            // On a phone the units are lanes, stacked (as the Mod / Motion
            // page's): pre modulation over the tremolo.
            if part == PrePart::Units && tier <= crate::control::Tier::Phone {
                div { style: "display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0;",
                    for (b, f, _) in items.iter().cloned() {
                        if let Some(f) = f {
                            div { key: "{b.id}", style: "flex: 1 1 0%; min-height: 0; display: flex;",
                                BlockFace { block: b.clone(), face: f, fill: true, preset_type: Some(b.block_type.as_str().to_string()), tempo_bpm: Some(tempo_bpm), algos: unit_algos(&b) }
                            }
                        }
                    }
                }
            } else {
            // Pre modulation, the tremolo, then the lanes at the right.
            for (b, f, extra) in items {
                if let Some(f) = f {
                    // The pedal, then what its block is doing — a delay's
                    // repeats, a reverb's tail. A tremolo shows its pulse on
                    // its own lamp.
                    Panel { key: "{b.id}", weight: if shows_viz(&b) || self_viewing(&f) || is_mod(&b) { 1.0 } else { 0.0 },
                        BlockFace { block: b.clone(), face: f.clone(), extra: extra.clone(), fill: self_viewing(&f) || is_mod(&b), stepper: true }
                        if shows_viz(&b) && !self_viewing(&f) {
                            div { style: "flex: 1 1 0%; min-width: 0; height: 100%; position: relative; display: flex; justify-content: center; border-left: 1px solid #1d1f24;",
                                // A reverb's own picture from frame (its
                                // springs, the algorithm it runs), else the
                                // effect's visualiser.
                                if let Some(p) = (b.block_type == BlockType::Reverb).then(|| faces.picture("reverb").cloned()).flatten() {
                                    BlockFace { block: b.clone(), face: p }
                                } else {
                                    {viz_of(&b, tempo_bpm)}
                                }
                            }
                        }
                    }
                } else {
                    BlockPanel { key: "{b.id}", block: b.clone(), face: None, card: PEDAL }
                }
            }
            }
            if !lanes.is_empty() {
                // One Time lane's width, pinned to the right: the bottom row
                // gives its Time module 65 % of its 1.15-share half, a lane
                // half of that (32.5 % × 1.15 / 2 = 18.69 % of the group),
                // and this row is 36.7 % of the group (control.rs):
                // 18.69 / 36.7.
                div { style: "flex: {lane_flex}; min-width: 0; height: 100%; display: flex; flex-direction: column; border-left: 1px solid #1d1f24;",
                    for (b, f, extra) in lanes {
                        div { key: "{b.id}", style: "flex: 1 1 0%; min-height: 0; display: flex;",
                            BlockFace { block: b.clone(), face: f, extra, fill: true, stepper: true, preset_type: Some(b.block_type.as_str().to_string()), tempo_bpm: Some(tempo_bpm) }
                        }
                    }
                }
            }
        }
    }
}

/// What of the Pre FX row to draw.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PrePart {
    #[default]
    All,
    /// The units: modulation and the tremolo.
    Units,
    /// The lanes: the delay over the reverb.
    Lanes,
}

/// A pre unit's menu from its name: a chorus's engines, a tremolo's modes.
fn unit_algos(b: &LiveBlock) -> Option<(&'static str, Vec<&'static str>)> {
    match b.block_type {
        BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato => Some(("engine", crate::control::MOD_ENGINES.to_vec())),
        BlockType::Trem => Some(("mode", crate::control::TREM_MODES.to_vec())),
        _ => None,
    }
}

/// Whether a pre effect's panel carries a picture beside its pedal (a
/// spring reverb's face shows its own springs).
fn shows_viz(b: &LiveBlock) -> bool {
    b.block_type == BlockType::Delay
}

/// What a pre effect is doing, drawn by the effect's own visualiser.
fn viz_of(b: &LiveBlock, tempo_bpm: u32) -> Element {
    match b.block_type {
        BlockType::Delay => rsx! { crate::control::DelayBlockViz { block: b.clone(), tempo_bpm } },
        BlockType::Reverb => rsx! { crate::control::ReverbBlockViz { block: b.clone(), tempo_bpm } },
        _ => rsx! { crate::control::ModBlockViz { block: b.clone() } },
    }
}

/// A unit's face without a block in the chain yet: drawn, not bound, and
/// off — so nothing on it moves.
#[component]
fn UnboundFace(
    face: FaceEntry,
    /// Fill the box, laid out for it (frame's responsive mode).
    #[props(default)]
    fill: bool,
    /// The block it stands for (name, type), to show its preset in the
    /// corner.
    #[props(default)]
    stepper: Option<(String, String)>,
) -> Element {
    let (w, h) = face.size;
    let off = vec![(format!("{}/on", face.ns), 0.0)];
    let boxed = if fill { "width: 100%; height: 100%; min-width: 0; min-height: 0;".to_string() } else { format!("height: 100%; aspect-ratio: {w} / {h}; flex: 0 0 auto; min-height: 0;") };
    rsx! {
        div { style: "position: relative; {boxed} opacity: 0.9;",
            if let Some((block, block_type)) = stepper {
                PresetCorner { block, block_type }
            }
            FrameSurface { name: face.face.clone(), values: off, responsive: fill }
        }
    }
}

/// The input stage's top row: pitch, the wah, and the envelope filter with
/// a picture of what it is doing. A unit whose block is not in the chain
/// yet is drawn but not bound.
#[component]
pub fn InputRow(
    blocks: Vec<LiveBlock>,
    /// Drawn right after the envelope filter: the input compressor.
    #[props(default = VNode::empty())]
    after_filter: Element,
    /// Only these units (`transpose`, `dive`, `pitch`, `doubler`, `wah`,
    /// `filter`) — a page of the phone's chain; empty for all.
    #[props(default)]
    only: Vec<String>,
) -> Element {
    let faces = use_faces();
    let want = |k: &str| only.is_empty() || only.iter().any(|o| o == k);
    let unit = |kind: &str, bt: BlockType| -> (Option<LiveBlock>, Option<FaceEntry>) {
        (blocks.iter().find(|b| b.block_type == bt).cloned(), pick(&faces.input, kind).cloned())
    };
    // The pitch blocks by name: the transposer and the dive bomb are pitch
    // blocks too.
    let named = |kind: &str, name: &str| -> (Option<LiveBlock>, Option<FaceEntry>) {
        (blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name)).cloned(), pick(&faces.input, kind).cloned())
    };
    let (pitch, pitch_face) = named("pitch", "Pitch");
    let (transpose, transpose_face) = named("transpose", "Transpose");
    let (dive, dive_face) = named("dive", "Dive Bomb");
    let (wah, wah_face) = unit("wah", BlockType::Wah);
    let (filter, filter_face) = unit("filter", BlockType::Filter);
    // The doubler: no block of its own yet — its unit, drawn.
    let doubler_face = pick(&faces.input, "doubler").cloned();
    let picture = faces.filter_picture.clone();
    // On a phone a page's units are lanes, stacked, in the page's order:
    // each its face's phone tier, its preset on its nameplate.
    let tier = crate::control::use_tier();
    if tier <= crate::control::Tier::Phone && !only.is_empty() {
        let lanes: Vec<(String, Option<LiveBlock>, Option<FaceEntry>)> = only
            .iter()
            .map(|k| {
                let (b, f) = match k.as_str() {
                    "transpose" => (transpose.clone(), transpose_face.clone()),
                    "dive" => (dive.clone(), dive_face.clone()),
                    "pitch" => (pitch.clone(), pitch_face.clone()),
                    "doubler" => (None, doubler_face.clone()),
                    "wah" => (wah.clone(), wah_face.clone()),
                    "filter" => (filter.clone(), filter_face.clone()),
                    "harmony" => named("harmony", "Harmonizer"),
                    "volume" => named("volume", "Volume Pedal"),
                    _ => (None, None),
                };
                (k.clone(), b, f.map(|f| f.at(tier)))
            })
            .collect();
        // The pedal pages are a board: the pedals side by side at their
        // own proportions, all one scale, the largest that fits — as the
        // drives. The rest are lanes, stacked.
        let board = only.iter().any(|k| matches!(k.as_str(), "wah" | "dive" | "volume"));
        if board {
            let (ww, wh) = try_use_context::<crate::control::WindowSize>().map_or((874.0, 381.0), |s| (s.0)());
            // The page: the window less the rail, the housing, the bars.
            let (room_w, room_h) = (ww - 60.0 - 54.0 - 24.0, wh - 44.0 - 30.0 - 30.0);
            let sizes: Vec<(f64, f64)> = lanes.iter().map(|(_, _, f)| f.as_ref().map_or((1.0, 1.0), |f| (f.size.0, f.size.1))).collect();
            let gaps = 14.0 * (sizes.len().saturating_sub(1)) as f64;
            let k = ((room_w - gaps) / sizes.iter().map(|s| s.0).sum::<f64>().max(1.0)).min(room_h / sizes.iter().map(|s| s.1).fold(1.0, f64::max));
            return rsx! {
                div { style: "display: flex; flex-direction: row; align-items: center; justify-content: center; gap: 14px; width: 100%; height: 100%; min-height: 0; background: linear-gradient(180deg, #121316, #0b0c0e); border-top: 3px solid #1c1d21; border-bottom: 3px solid #1c1d21;",
                    for ((k_, b, f), (fw, fh)) in lanes.into_iter().zip(sizes) {
                        div { key: "{k_}", style: "flex: 0 0 auto; width: {fw * k}px; height: {fh * k}px; display: flex; filter: drop-shadow(0 6px 8px rgba(0,0,0,0.6));",
                            match (b, f) {
                                (Some(b), Some(f)) => rsx! { BlockFace { block: b.clone(), face: f, fill: true, preset_type: Some(b.block_type.as_str().to_string()) } },
                                (None, Some(f)) => rsx! { UnboundFace { face: f, fill: true } },
                                _ => rsx! {},
                            }
                        }
                    }
                }
            };
        }
        // The Pitch page: the octaver a third, the harmonizer the rest, side
        // by side; the rest stacked.
        let row = only.iter().any(|k| k == "harmony");
        let dir = if row { "row" } else { "column" };
        let grow = |k: &str| if k == "harmony" { 2 } else { 1 };
        return rsx! {
            div { style: "display: flex; flex-direction: {dir}; gap: 6px; width: 100%; height: 100%; min-height: 0; min-width: 0;",
                for (k, b, f) in lanes {
                    div { key: "{k}", style: "flex: {grow(&k)} 1 0%; min-height: 0; min-width: 0; display: flex;",
                        match (b, f) {
                            (Some(b), Some(f)) => rsx! { BlockFace { block: b.clone(), face: f, fill: true, preset_type: Some(b.block_type.as_str().to_string()) } },
                            (None, Some(f)) => rsx! { UnboundFace { face: f, fill: true } },
                            _ => rsx! {},
                        }
                    }
                }
            }
        };
    }
    rsx! {
        Row {
            // The transposer, first in the chain, then the dive bomb.
            for (b, f, name, kind) in [(transpose, transpose_face, "Transpose", "transpose"), (dive, dive_face, "Dive Bomb", "dive")].into_iter().filter(|u| want(u.3)) {
                Panel { weight: if only.is_empty() { 0.0 } else { 1.0 },
                    match (b, f) {
                        (Some(b), Some(f)) => rsx! { BlockFace { block: b, face: f, stepper: true } },
                        (None, Some(f)) => rsx! { UnboundFace { face: f, stepper: Some((name.to_string(), kind.to_string())) } },
                        _ => rsx! {},
                    }
                }
            }
            // The pitch stage: its voices on their ladder, the doubler —
            // the widest unit here, laid out for its room.
            if want("pitch") {
            Panel { weight: 1.3,
                match (pitch, pitch_face) {
                    (Some(b), Some(f)) => rsx! { BlockFace { block: b, face: f, fill: true, stepper: true } },
                    (None, Some(f)) => rsx! { UnboundFace { face: f, fill: true, stepper: Some(("Pitch".to_string(), "pitch".to_string())) } },
                    _ => rsx! { div { style: "width: 280px; height: 100%; display: flex;", crate::control::PitchStrip {} } },
                }
            }
            }
            // The doubler, after the voices.
            if let Some(f) = doubler_face.filter(|_| want("doubler")) {
                Panel { weight: 0.5,
                    UnboundFace { face: f, fill: true }
                }
            }
            if want("wah") {
            Panel { weight: if only.is_empty() { 0.0 } else { 1.0 },
                match (wah, wah_face) {
                    (Some(b), Some(f)) => rsx! { BlockFace { block: b, face: f, stepper: true } },
                    (None, Some(f)) => rsx! { UnboundFace { face: f, stepper: Some(("Wah".to_string(), "wah".to_string())) } },
                    _ => rsx! {},
                }
            }
            }
            // The envelope filter: one unit, its screen inside, filling the
            // room left it (laid out for it).
            if want("filter") {
            Panel { weight: 0.9,
                match (filter.clone(), filter_face) {
                    (Some(b), Some(f)) => rsx! { BlockFace { block: b, face: f, fill: true, stepper: true } },
                    (None, Some(f)) => rsx! { UnboundFace { face: f, fill: true, stepper: Some(("Filter".to_string(), "filter".to_string())) } },
                    _ => rsx! {},
                }
                if let Some(p) = picture {
                    div { style: "flex: 1 1 0%; min-width: 0; height: 100%; display: flex; border-left: 1px solid #1d1f24;",
                        div { style: "width: 100%; height: 100%;",
                            FrameSurface { name: p.face.clone(), stretch: true, values: filter.as_ref().map_or_else(|| vec![(format!("{}/on", p.ns), 0.0)], |b| values_of(b, &p.ns)) }
                        }
                    }
                }
            }
            }
            {after_filter}
        }
    }
}

/// An amp's cab, when one is loaded: `Amp L`'s is `Cab L`, and a cab is
/// loaded when it names an impulse (its preset is the IR's name).
fn cab_of(blocks: &[LiveBlock], amp: &LiveBlock) -> Option<LiveBlock> {
    let side = amp.name.rsplit(' ').next().unwrap_or("L");
    blocks
        .iter()
        .find(|b| b.block_type == BlockType::Cabinet && b.name.eq_ignore_ascii_case(&format!("Cab {side}")) && !b.preset.is_empty())
        .cloned()
}
