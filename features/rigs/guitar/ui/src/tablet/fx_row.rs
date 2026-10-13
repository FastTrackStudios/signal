//! The FX row — the selected block's controls, as Frame draws them: its
//! face from the rig-faces set (`rig_faces`), laid out for the row; a
//! module's blocks side by side; a block with no face its name card. With
//! nothing picked in the routing strip, the drive board's first pedal.

use dioxus::prelude::*;
use signal_guitar_proto::LiveBlock;
use signal_proto::block::BlockType;

use super::routing::{RoutingSel, Selected};
use super::tokens::*;
use crate::rig_faces::{use_faces, BlockFace, FaceEntry, Faces, NameCard};
use crate::state::RigViewState;

/// The row's box, pt — what a landscape iPhone 16 Pro leaves for it: its
/// 750 × 381 safe area less the phone's top bar and routing strip, so one
/// face set serves this row and a phone on its side.
pub const FX_W: f64 = 750.0;
pub const FX_H: f64 = 254.0;

/// A pre effect (before the amp): its face is its pedal's.
fn is_pre(b: &LiveBlock) -> bool {
    b.name.to_lowercase().starts_with("pre ")
}

/// An input-stage block (ahead of the drives): the octaver and harmonizer,
/// the envelope filter or wah, the volume pedal, the dive bomb, the
/// transposer — each wears its own unit.
fn is_input(b: &LiveBlock) -> bool {
    let name = b.name.to_lowercase();
    ["pitch", "harmon", "octav", "filter", "wah", "volume pedal", "dive", "transpose", "doubler"].iter().any(|w| name.contains(w))
}

/// The face a block wears, if the set has one: a drive slot's pedal, an
/// amp's capture, the gate, the EQ, the compressors, a delay machine or
/// reverb algorithm, a modulation effect, a pre effect's pedal, the trim.
pub fn face_for(b: &LiveBlock, faces: &Faces) -> Option<FaceEntry> {
    if b.empty {
        return None;
    }
    let pedal = || faces.pre(&b.preset).or_else(|| faces.pre(&b.name)).cloned();
    let param = |name: &str| b.params.iter().find(|p| p.name == name).map(|p| p.value.round().max(0.0) as usize);
    if is_input(b) {
        return faces.input_for(&b.preset).or_else(|| faces.input_for(&b.name)).cloned();
    }
    match b.block_type {
        // A drive slot: its pedal, by the preset's name or the capture's.
        BlockType::Drive | BlockType::Boost if !is_pre(b) => faces.drive(&b.preset).or_else(|| faces.drive(&b.detail)).cloned().or_else(pedal),
        BlockType::Amp => faces.amp(&b.asset).or_else(|| faces.amp(&b.preset)).cloned(),
        BlockType::Cabinet => faces.cab.clone(),
        BlockType::Gate => faces.gate.clone(),
        BlockType::Eq => faces.eq.clone(),
        // The Pre Comp: the compressor's scope (its curve, its gain
        // reduction), as its block in the grid shows it.
        BlockType::Compressor if b.name.to_lowercase().contains("pre") => faces.comp.clone().or_else(|| faces.pre_comp_pedal.clone()).or_else(pedal),
        // The post compressor is the Distressor; any other (the master's
        // limiter) the compressor scope.
        BlockType::Compressor if b.name.to_lowercase().contains("post") => faces.post_comp.clone(),
        BlockType::Compressor => faces.comp.clone().or_else(|| faces.post_comp.clone()),
        // A delay or reverb: its machine's face (a pre one: its pedal, if
        // its preset has one).
        BlockType::Delay => (if is_pre(b) { pedal() } else { None })
            .or_else(|| faces.time(false, crate::control::DELAY_ALGOS.get(param("style").unwrap_or(1)).copied().unwrap_or("")).cloned()),
        BlockType::Reverb => (if is_pre(b) { pedal() } else { None })
            .or_else(|| faces.time(true, crate::control::VERB_ALGOS.get(param("algorithm").unwrap_or(1)).copied().unwrap_or("")).cloned()),
        // A level (the patch's trim, the post-amp boost): the level face.
        BlockType::Volume if b.params.iter().any(|p| p.name == "gain_db") => faces.trim.clone(),
        // A pre effect: its pedal, else its type's face (a pre tremolo the
        // tremolo's).
        t if is_pre(b) => pedal().or_else(|| faces.modulation(t.as_str()).cloned()),
        t => faces.modulation(t.as_str()).cloned().or_else(pedal),
    }
}

/// An input-stage slot the patch leaves empty (`slot:Filter`), as a block
/// of its kind, off: the row shows the unit it would be.
fn slot_block(id: &str) -> Option<LiveBlock> {
    let (name, block_type) = match id.strip_prefix("slot:")? {
        "Filter" => ("Filter", BlockType::Filter),
        "Pitch" => ("Pitch", BlockType::Pitch),
        "Comp" => ("Pre Comp", BlockType::Compressor),
        _ => return None,
    };
    Some(LiveBlock {
        id: id.to_string(),
        engine: 0,
        block_type,
        name: name.to_string(),
        bypassed: true,
        param_name: None,
        param_value: 0.0,
        param_min: 0.0,
        param_max: 1.0,
        output_level_db: None,
        detail: String::new(),
        asset: String::new(),
        module: String::new(),
        empty: false,
        params: Vec::new(),
        preset: String::new(),
        options: Vec::new(),
        option: 0,
        overridden: false,
    })
}

/// A block's live readings for its face, by its namespace: the spectrum
/// under an EQ, a compressor's level and gain reduction and their traces,
/// a gate's level, the Distressor's lamps.
fn streams_for(b: &LiveBlock, ns: &str, state: &RigViewState) -> Vec<(String, Vec<f64>)> {
    let db = |lin: f32| if lin <= 1e-6 { -90.0 } else { f64::from(20.0 * lin.log10()).max(-90.0) };
    let in_db = f64::from(*state.in_peak_db.read());
    match b.block_type {
        BlockType::Eq => {
            let spectrum: Vec<f64> = state.spectrum.read().iter().map(|v| f64::from(*v)).collect();
            if spectrum.is_empty() { Vec::new() } else { vec![(format!("{ns}/spectrum"), spectrum)] }
        }
        BlockType::Compressor | BlockType::Gate => {
            let waves = state.comp_wave.read();
            let trace = waves.get(&b.name);
            let gr_db = trace.map_or_else(|| f64::from(*state.comp_gr_db.read()), |t| f64::from(t.2));
            let mut out = vec![(format!("{ns}/in"), vec![in_db]), (format!("{ns}/gr"), vec![-gr_db.abs()])];
            match trace {
                Some((input, gr, _)) => {
                    out.push((format!("{ns}/in_history"), input.iter().map(|v| db(*v)).collect()));
                    out.push((format!("{ns}/gr_history"), gr.iter().map(|v| -f64::from(*v) * 40.0).collect()));
                }
                // No trace of its own (a gate): its level, a reading at a time.
                None => out.push((format!("{ns}/in_history"), vec![in_db])),
            }
            out
        }
        _ => Vec::new(),
    }
}

#[component]
pub fn FxRow(state: RigViewState) -> Element {
    let sel = use_context::<RoutingSel>().0;
    let faces = use_faces();
    let blocks = state.blocks.read().clone();
    // What it shows: a block picked, alone, the whole row its own; a module
    // picked, its blocks side by side; nothing picked, the drive board's
    // first pedal.
    let first = || blocks.iter().find(|b| b.block_type == BlockType::Drive && !b.empty).or_else(|| blocks.first()).map(|b| b.id.clone());
    let shown: Vec<LiveBlock> = match sel().or_else(|| first().map(Selected::Block)) {
        // A block: the whole row its own. An empty slot of the input stage
        // (the patch has no filter, say): its unit, off.
        Some(Selected::Block(id)) => blocks.iter().filter(|b| b.id == id).cloned().collect::<Vec<_>>().into_iter().next().or_else(|| slot_block(&id)).into_iter().collect(),
        Some(Selected::Module(_, ids)) => blocks.iter().filter(|b| ids.contains(&b.id)).cloned().collect(),
        None => Vec::new(),
    };
    let one = shown.len() == 1;
    let face_h = FX_H;
    // The row's width, measured: one block's face fills it edge to edge.
    let mut row_w = use_signal(|| FX_W);
    let measure = move |el: std::rc::Rc<MountedData>| {
        spawn(async move {
            for _ in 0..10 {
                if let Ok(r) = el.get_client_rect().await
                    && r.width() > 0.0
                {
                    if (*row_w.peek() - r.width()).abs() > 0.5 {
                        row_w.set(r.width());
                    }
                    return;
                }
                architect::platform::sleep(std::time::Duration::from_millis(30)).await;
            }
        });
    };
    let w = row_w();
    // A module's faces at the row's height side by side, each in its own
    // proportion — and, when they are wider than the row together, all
    // narrowed by the same factor so the row holds every one whole (wider,
    // centred, the first would spill off the left where no scroll reaches).
    let natural = |b: &LiveBlock| match face_for(b, &faces) {
        Some(f) => {
            let f = f.at(crate::control::Tier::Ipad);
            (face_h * f.size.0 / f.size.1.max(1.0)).round()
        }
        None => 260.0,
    };
    let total: f64 = shown.iter().map(natural).sum::<f64>() + (shown.len().saturating_sub(1)) as f64;
    let fit = if !one && total > w { (w / total).min(1.0) } else { 1.0 };
    rsx! {
        div { style: "flex-shrink: 0; height: {FX_H}px; box-sizing: border-box; border-top: 1px solid {WELL}; background: {SHEET}; display: flex; flex-direction: column;",
            div { style: "flex: 1; min-height: 0; overflow-x: auto; overflow-y: hidden;",
            onmounted: move |e| measure(e.data()),
            div { style: "height: 100%; display: flex; align-items: stretch; justify-content: center; gap: 1px; width: 100%;",
                for b in shown.clone().into_iter() {
                    {
                        let face = face_for(&b, &faces);
                        rsx! {
                            div { key: "{b.id}", style: "position: relative; flex-shrink: 0; height: 100%; display: flex; align-items: stretch;",
                                match face {
                                    // One block: its face fills the row's box.
                                    Some(f) if one => rsx! {
                                        div { style: "width: {w}px; height: 100%;",
                                            BlockFace { block: b.clone(), streams: streams_for(&b, &f.ns, &state), face: f.for_row(w, face_h), fill: true }
                                        }
                                    },
                                    // A module's: each in a box the row's height
                                    // and its own proportion, so each fits whole.
                                    Some(f) => {
                                        let f = f.at(crate::control::Tier::Ipad);
                                        let w = (face_h * f.size.0 / f.size.1.max(1.0) * fit).floor();
                                        rsx! {
                                            div { style: "width: {w}px; height: 100%;",
                                                BlockFace { block: b.clone(), streams: streams_for(&b, &f.ns, &state), face: f, fill: true }
                                            }
                                        }
                                    }
                                    // No face of its own: a drive's or an amp's cover
                                    // art (its capture's, from TONE3000), else its
                                    // name card.
                                    None => rsx! {
                                        div { style: "width: {pick(one, w, (260.0 * fit).floor())}px; height: 100%; padding: 12px; box-sizing: border-box;",
                                            ArtCard { block: b.clone(), aspect: (pick(one, w, (260.0 * fit).floor()), face_h) }
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
            }
        }
    }
}

/// What the FX row shows, and its preset — in the foot bar's middle, so the
/// row keeps its height. A drive or an amp (or a module picked): its module
/// preset and variation, ‹ › through the variations, a dot when it plays
/// changed, ⋯ to save it (to the variation, as a new one, as a new preset),
/// rename or delete. Any other block: its block preset, the same ⋯. The
/// name turns the sidebar into the presets.
#[component]
pub fn FxPresetBar(state: RigViewState) -> Element {
    use super::menu::{Item, MoreButton, Picked};
    use super::setlist::call;
    let rig = use_hook(try_consume_context::<signal_guitar_proto::rig::RigClient>);
    let sel = use_context::<RoutingSel>().0;
    let blocks = state.blocks.read().clone();
    let first = || blocks.iter().find(|b| b.block_type == BlockType::Drive && !b.empty).or_else(|| blocks.first()).map(|b| b.id.clone());
    let comp = try_use_context::<crate::face_chrome::FacePresets>().map(|c| c.0.read().clone()).unwrap_or_default();
    let open = try_use_context::<crate::face_chrome::OpenPresets>();
    let picked_module = match sel() {
        Some(Selected::Module(m, _)) => Some(m),
        _ => None,
    };
    let bar_block = if picked_module.is_some() {
        None
    } else {
        match sel().or_else(|| first().map(Selected::Block)) {
            Some(Selected::Block(id)) => blocks.iter().find(|b| b.id == id).cloned().or_else(|| slot_block(&id)),
            _ => None,
        }
    };
    // The module it saves into: the one picked, or a drive's or amp's own.
    let module_pick = match (&picked_module, &bar_block) {
        (Some(m), _) => comp.active_modules.iter().find(|p| p.module.eq_ignore_ascii_case(m)).cloned(),
        (None, Some(b)) if matches!(b.block_type, BlockType::Drive | BlockType::Boost | BlockType::Amp) => {
            let owns = |p: &&signal_guitar_proto::ModulePick| p.blocks.iter().any(|x| x.eq_ignore_ascii_case(&b.name));
            // Its own module (Drive, Amp) before the Core that holds them all.
            comp.active_modules.iter().filter(owns).find(|p| !p.module.eq_ignore_ascii_case("Core")).or_else(|| comp.active_modules.iter().find(owns)).cloned()
        }
        _ => None,
    };
    let label = picked_module.clone().or_else(|| bar_block.as_ref().map(|b| b.name.clone())).unwrap_or_default();
    let tag = format!("font-size: 12px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {INK_2}; white-space: nowrap;");
    let chip = format!("height: {HIT}px; max-width: 300px; display: flex; align-items: center; gap: 8px; padding: 0 14px; border: none; border-radius: {R}; background: {FILL_ON}; color: {INK}; font-size: 15px; font-weight: 650; white-space: nowrap; overflow: hidden; cursor: pointer;");
    let arrow = format!("width: {HIT}px; height: {HIT}px; display: flex; align-items: center; justify-content: center; border: none; border-radius: {R}; background: transparent; cursor: pointer;");

    if let Some(pick) = module_pick {
        let module = pick.module.clone();
        let edited = blocks.iter().any(|b| b.overridden && pick.blocks.iter().any(|x| x.eq_ignore_ascii_case(&b.name)));
        let entry = comp.modules.iter().find(|m| m.module.eq_ignore_ascii_case(&module) && m.name == pick.preset).cloned();
        let variations = entry.as_ref().map(|e| e.snapshots.clone()).unwrap_or_default();
        let presets: Vec<String> = comp.modules.iter().filter(|m| m.module.eq_ignore_ascii_case(&module)).map(|m| m.name.clone()).collect();
        let shown = if pick.snapshot.is_empty() { pick.preset.clone() } else { format!("{} · {}", pick.preset, pick.snapshot) };
        let var_name = if pick.snapshot.is_empty() { "Main".to_string() } else { pick.snapshot.clone() };
        let items = vec![
            Item::head(shown.clone()),
            Item::run("update", format!("Save to {var_name}")).unless((!edited).then(|| "Nothing changed".to_string())),
            Item::name("new_var", "New variation…", format!("{var_name} 2"), "Save", variations.clone()),
            Item::name("new_preset", "New preset…", format!("{} 2", pick.preset), "Save", presets.clone()),
            Item::name("ren_var", "Rename variation…", var_name.clone(), "Rename", variations.clone()),
            Item::name("ren_preset", "Rename preset…", pick.preset.clone(), "Rename", presets),
            Item::Sep,
            Item::delete("del_var", "Delete variation").unless((variations.len() <= 1).then(|| "Its only variation".to_string())),
            Item::delete("del_preset", "Delete preset"),
        ];
        let on_pick = {
            let (rig, module, preset, var) = (rig.clone(), module.clone(), pick.preset.clone(), var_name.clone());
            EventHandler::new(move |x: Picked| {
                let (m, p, v, t) = (module.clone(), preset.clone(), var.clone(), x.text.clone());
                match x.id.as_str() {
                    "update" => call!(rig, |r| r.save_module_snapshot(m, p, v)),
                    "new_var" => call!(rig, |r| r.save_module_snapshot(m, p, t)),
                    "new_preset" => call!(rig, |r| r.save_module_snapshot(m, t, v)),
                    "ren_var" => call!(rig, |r| r.rename_module_snapshot(m, p, v, t)),
                    "ren_preset" => call!(rig, |r| r.rename_module_preset(m, p, t)),
                    "del_var" => call!(rig, |r| r.delete_module_snapshot(m, p, v)),
                    "del_preset" => call!(rig, |r| r.delete_module_preset(m, p)),
                    _ => {}
                }
            })
        };
        let (r1, r2, m1, m2) = (rig.clone(), rig.clone(), module.clone(), module.clone());
        let (open_name, open_kind) = (label.clone(), format!("module:{module}"));
        return rsx! {
            div { style: "flex: 1; min-width: 0; display: flex; align-items: center; justify-content: center; gap: 6px; padding: 0 12px;",
                span { style: "{tag}", "{label}" }
                button { style: "{arrow}", onclick: move |_| { let m = m1.clone(); call!(r1, |r| r.step_module(m, -1)); }, Chevron { left: true } }
                button {
                    style: "{chip}",
                    onclick: move |_| {
                        if let Some(crate::face_chrome::OpenPresets(o)) = open {
                            o.call((open_name.clone(), open_kind.clone()));
                        }
                    },
                    span { style: "overflow: hidden; text-overflow: ellipsis;", "{shown}" }
                    if edited {
                        span { style: "width: 8px; height: 8px; border-radius: 4px; background: #f59e0b; flex-shrink: 0;" }
                    }
                }
                button { style: "{arrow}", onclick: move |_| { let m = m2.clone(); call!(r2, |r| r.step_module(m, 1)); }, Chevron { left: false } }
                MoreButton { label: format!("{module} preset actions"), items, on_pick }
            }
        };
    }

    // A block of its own: its block preset.
    let Some(b) = bar_block else {
        return rsx! { span { style: "flex: 1;" } };
    };
    let kind = b.block_type.as_str().to_lowercase();
    let mine: Vec<String> = comp.block_presets.iter().filter(|p| p.block_type.eq_ignore_ascii_case(&kind)).map(|p| p.name.clone()).collect();
    let playing = comp.active_blocks.iter().find(|x| x.block.eq_ignore_ascii_case(&b.name)).map(|x| x.preset.clone()).unwrap_or_default();
    let mut items = vec![Item::head(if playing.is_empty() { b.name.clone() } else { playing.clone() })];
    if !playing.is_empty() {
        items.push(Item::run("update", format!("Save to {playing}")).unless((!b.overridden).then(|| "Nothing changed".to_string())));
    }
    items.push(Item::name("new", "New preset…", if playing.is_empty() { format!("{} 1", b.name) } else { format!("{playing} 2") }, "Save", mine.clone()));
    if !playing.is_empty() {
        items.push(Item::name("rename", "Rename…", playing.clone(), "Rename", mine.clone()));
        items.push(Item::Sep);
        items.push(Item::delete("delete", "Delete preset"));
    }
    let on_pick = {
        let (rig, block, playing) = (rig.clone(), b.name.clone(), playing.clone());
        EventHandler::new(move |x: Picked| {
            let (blk, p, t) = (block.clone(), playing.clone(), x.text.clone());
            match x.id.as_str() {
                "update" => call!(rig, |r| r.save_block_preset(blk, p)),
                "new" => call!(rig, |r| r.save_block_preset(blk, t)),
                "rename" => call!(rig, |r| r.rename_block_preset(p, t)),
                "delete" => call!(rig, |r| r.delete_block_preset(p)),
                _ => {}
            }
        })
    };
    rsx! {
        div { style: "flex: 1; min-width: 0; display: flex; align-items: center; justify-content: center; gap: 6px; padding: 0 12px;",
            span { style: "{tag}", "{label}" }
            crate::face_chrome::PresetStepper { block: b.name.clone(), block_type: kind, show_empty: true, touch: true }
            if b.overridden {
                span { style: "width: 8px; height: 8px; border-radius: 4px; background: #f59e0b;" }
            }
            MoreButton { label: format!("{} preset actions", b.name), items, on_pick }
        }
    }
}

/// A block's cover art (a TONE3000 capture's photograph) filling its card,
/// its name and preset on a band along the foot; its name card while there
/// is none.
#[component]
fn ArtCard(block: LiveBlock, aspect: (f64, f64)) -> Element {
    let rig = use_hook(try_consume_context::<signal_guitar_proto::rig::RigClient>);
    let has_art = matches!(block.block_type, BlockType::Drive | BlockType::Boost | BlockType::Amp);
    let name = block.name.clone();
    let preset = block.preset.clone();
    let art = use_resource(use_reactive!(|name, preset| {
        let rig = rig.clone();
        async move {
            let _ = preset;
            if !has_art {
                return None;
            }
            let a = rig?.block_artwork(name).await.ok()?;
            if a.bytes.is_empty() {
                return None;
            }
            use base64::Engine as _;
            Some(format!("data:{};base64,{}", a.mime, base64::engine::general_purpose::STANDARD.encode(&a.bytes)))
        }
    }));
    match art.read().clone().flatten() {
        Some(uri) => rsx! {
            div { style: "position: relative; width: 100%; height: 100%; border-radius: {R_MD}; overflow: hidden; background: #111;",
                img { src: "{uri}", style: "position: absolute; left: 0; top: 0; width: 100%; height: 100%; object-fit: cover;" }
                div { style: "position: absolute; left: 0; right: 0; bottom: 0; display: flex; align-items: baseline; gap: 10px; padding: 10px 14px; background: rgba(0,0,0,0.6);",
                    span { style: "font-size: 13px; font-weight: 800; letter-spacing: 0.06em; text-transform: uppercase; color: {INK_2};", "{block.name}" }
                    span { style: "font-size: 16px; font-weight: 700; color: {INK}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{block.preset}" }
                }
            }
        },
        None => rsx! { NameCard { block, aspect } },
    }
}

/// A step's chevron, drawn (a glyph set in the font sits off-centre and
/// changes weight with it).
#[component]
pub(super) fn Chevron(left: bool) -> Element {
    rsx! {
        svg { width: "10", height: "16", view_box: "0 0 10 16",
            path { d: pick(left, "M8 2 2 8l6 6", "M2 2l6 6-6 6"), fill: "none", stroke: FOCUS_FG, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
        }
    }
}
