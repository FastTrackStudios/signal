//! The rig's time effects: a face for every delay machine and every reverb
//! algorithm of signal's Delay and Reverb blocks, each its own unit — the
//! machine that algorithm is (a tape echo, a bucket-brigade box, a steel
//! plate, a hall processor…), with a live picture of what it is doing and
//! its own controls.
//!
//! Each face fills one lane of signal's Time module: [`kit::W`] ×
//! [`kit::H`], two stacked in the Delay panel (DLY 1, DLY 2) and two in the
//! Reverb panel. A block's `style` (delay) or `algorithm` (reverb) picks the
//! face; switching it on the face's selector swaps the face.
//!
//! Binding: every delay face's params are `dly/<block param>`, every
//! reverb's `verb/<block param>` (see [`kit::delay_params`] /
//! [`kit::reverb_params`]); `<ns>/on` is the block's bypass.

pub mod delay;
pub mod kit;
pub mod reverb;

use serde_json::json;

use crate::face::Face;

/// Every time face.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v = delay::faces(false);
    v.extend(delay::faces(true));
    v.extend(reverb::faces(false));
    v.extend(reverb::faces(true));
    v.extend(reverb::phone_faces(false).into_iter().map(|(_, f)| f));
    v.extend(delay::phone_faces(false).into_iter().map(|(_, f)| f));
    v.extend(delay::phone_faces(true).into_iter().map(|(_, f)| f));
    v.extend(delay::small_faces(false, kit::Tier::Ipad).into_iter().map(|(_, f)| f));
    v.extend(delay::small_faces(true, kit::Tier::Ipad).into_iter().map(|(_, f)| f));
    for dark in [false, true] {
        v.extend(delay::back_faces(dark, kit::Tier::Ipad).into_iter().map(|(_, f)| f));
        v.extend(reverb::pane_faces(dark).into_iter().map(|(_, f)| f));
        v.extend(reverb::back_faces(dark, kit::Tier::Ipad).into_iter().map(|(_, f)| f));
    }
    for t in [kit::Tier::Desktop, kit::Tier::Phone] {
        for dark in [false, true] {
            v.extend(reverb::back_faces(dark, t).into_iter().map(|(_, f)| f));
            v.extend(delay::back_faces(dark, t).into_iter().map(|(_, f)| f));
        }
    }
    v.extend(reverb::phone_faces(true).into_iter().map(|(_, f)| f));
    for dark in [false, true] {
        v.extend(reverb::tall_faces(dark));
        v.extend(delay::tall_faces(dark));
    }
    v.extend(machines());
    v
}

/// The machines a reverb can wear whatever its algorithm (the 480), every
/// shape and finish of each.
fn machines() -> Vec<Face> {
    use reverb::four80;
    let mut v = Vec::new();
    for dark in [false, true] {
        for t in [kit::Tier::Desktop, kit::Tier::Phone] {
            v.push(four80::face_at(dark, t));
            v.push(four80::back_at(dark, t));
        }
        v.push(four80::tall(dark));
    }
    v
}

/// `faces.json`'s `time` entry: each algorithm's face, by its name.
#[must_use]
/// A Time face's manifest entry: `e`, and what `f` carries — the
/// nameplate (the host fills the preset and answers its presses), the Time
/// knob (time, sync, time_text, the host maps), and its versions for other
/// rooms (`tiers`: the app draws the one for the room it is given).
fn entry(mut e: serde_json::Value, f: &Face, (tall, tall_dark): (&Face, &Face), phone: Option<(usize, Face)>, phone_dark: Option<(usize, Face)>, pane: Option<(Face, Face)>, backs: impl Fn(bool, kit::Tier) -> Option<Face>) -> serde_json::Value {
    // Its tall face, for standing alone in a lane column: everything on
    // it, two phone lanes high.
    e["tall"] = json!(tall.file);
    e["tall_dark"] = json!(tall_dark.file);
    e["tall_size"] = json!([kit::TALL_W, kit::TALL_H]);
    // What a face carries, for the host: its own nameplate, its own Time
    // knob — said of each version, since a small room's may carry them
    // where the base face does not (the digital rack).
    let carries = |f: &Face, e: &mut serde_json::Value| {
        if f.params.iter().any(|p| p.addr.to_string().ends_with("/preset_browse")) {
            e["nameplate"] = json!(true);
        }
        if f.params.iter().any(|p| p.addr.to_string().ends_with("/time_text")) {
            e["time_knob"] = json!(true);
        }
    };
    carries(f, &mut e);
    // Its back (the advanced controls), where it has one: the host turns
    // the face over to it on the flip arrow.
    if let (Some(bl), Some(bd)) = (backs(false, kit::Tier::Desktop), backs(true, kit::Tier::Desktop)) {
        e["back"] = json!(bl.file);
        e["back_dark"] = json!(bd.file);
    }
    if let (Some((_, pl)), Some((_, pd))) = (phone, phone_dark) {
        let mut t = json!({ "face": pl.file, "dark": pd.file, "size": [kit::Tier::Phone.lane_w(), kit::H] });
        carries(&pl, &mut t);
        if let (Some(bl), Some(bd)) = (backs(false, kit::Tier::Phone), backs(true, kit::Tier::Phone)) {
            t["back"] = json!(bl.file);
            t["back_dark"] = json!(bd.file);
        }
        e["tiers"] = json!({ "phone": t });
    }
    // A desktop pane's lane, where the face has one.
    if let Some((pl, pd)) = pane {
        let mut t = json!({ "face": pl.file, "dark": pd.file, "size": [kit::Tier::Ipad.lane_w(), kit::H] });
        carries(&pl, &mut t);
        if let (Some(bl), Some(bd)) = (backs(false, kit::Tier::Ipad), backs(true, kit::Tier::Ipad)) {
            t["back"] = json!(bl.file);
            t["back_dark"] = json!(bd.file);
        }
        if e.get("tiers").is_none() {
            e["tiers"] = json!({});
        }
        e["tiers"]["ipad"] = t;
    }
    e
}

pub fn manifest() -> serde_json::Value {
    let size = [kit::W, kit::H];
    let (dt, dtd, rt, rtd) = (delay::tall_faces(false), delay::tall_faces(true), reverb::tall_faces(false), reverb::tall_faces(true));
    let delays: Vec<_> = kit::DELAY_ALGOS
        .iter()
        .zip(delay::faces(false).into_iter().zip(delay::faces(true)))
        .enumerate()
        .map(|(i, (name, (f, d)))| {
            let pane = |dark: bool| delay::small_faces(dark, kit::Tier::Ipad).into_iter().find(|(at, _)| *at == i).map(|(_, f)| f);
            entry(json!({ "match": [name.to_lowercase()], "face": f.file, "dark": d.file, "ns": kit::DLY, "size": size }), &f, (&dt[i], &dtd[i]), delay::phone_faces(false).into_iter().find(|(at, _)| *at == i), delay::phone_faces(true).into_iter().find(|(at, _)| *at == i), pane(false).zip(pane(true)), |dark, t| delay::back_faces(dark, t).into_iter().find(|(at, _)| *at == i).map(|(_, f)| f))
        })
        .collect();
    let reverbs: Vec<_> = kit::VERB_ALGOS
        .iter()
        .zip(reverb::faces(false).into_iter().zip(reverb::faces(true)))
        .enumerate()
        .map(|(i, (name, (f, d)))| entry(json!({ "match": [name.to_lowercase()], "face": f.file, "dark": d.file, "ns": kit::VERB, "size": size }), &f, (&rt[i], &rtd[i]), reverb::phone_faces(false).into_iter().find(|(at, _)| *at == i), reverb::phone_faces(true).into_iter().find(|(at, _)| *at == i), reverb::pane_faces(false).into_iter().find(|(at, _)| *at == i).map(|(_, f)| f).zip(reverb::pane_faces(true).into_iter().find(|(at, _)| *at == i).map(|(_, f)| f)), |dark, t| reverb::back_faces(dark, t).into_iter().find(|(at, _)| *at == i).map(|(_, f)| f)))
        .collect();
    // Then the machines, by their own names: they replace no algorithm's
    // entry (the host looks an algorithm up by its name).
    let mut reverbs = reverbs;
    {
        use reverb::four80;
        let (d, p) = (kit::Tier::Desktop, kit::Tier::Phone);
        let backs = |dark: bool, t: kit::Tier| Some(four80::back_at(dark, t));
        reverbs.push(entry(json!({ "match": ["480"], "face": four80::face(false).file, "dark": four80::face(true).file, "ns": kit::VERB, "size": size }), &four80::face_at(false, d), (&four80::tall(false), &four80::tall(true)), Some((0, four80::face_at(false, p))), Some((0, four80::face_at(true, p))), Some((four80::face_at(false, kit::Tier::Ipad), four80::face_at(true, kit::Tier::Ipad))), backs));
    }
    json!({ "delay": delays, "reverb": reverbs })
}
