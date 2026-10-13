//! Hardware-style plugin faces built from `surface-kit`.
//!
//! Each module returns its [`Face`]s; [`all`] collects them and
//! `cargo run -p plugin-faces -- OUT_DIR` writes `OUT_DIR/<plugin>/<file>.frame`
//! plus `OUT_DIR/<plugin>/<plugin>.catalog.json`, so `frame OUT_DIR/<plugin>`
//! plays a plugin's faces as a gallery.

pub mod animate;
pub mod comp_fet;
pub mod comp_look;
pub mod comp_sc;
pub mod comp_tube;
pub mod comp_vca;
pub mod delay;
pub mod eq;
pub mod face;
pub mod lane;
pub mod modulation;
pub mod reverb;
pub mod rig_blues;
pub mod rig_faces;
pub mod rig_amp;
pub mod rig_mod;
pub mod rig_cells;
pub mod rig_units;
pub mod rig_time;
pub mod saturate;
pub mod suite_boutique_clean;
pub mod suite_funk_rhythm;
pub mod suite_studio_blues;
pub mod suite_poly_clean;
pub mod rigkit;
pub mod suite_blackout_gain;
pub mod suite_djent_forge;
pub mod suite_modern_rhythm;

pub use face::{Face, canvas, catalogs};

#[must_use]
pub fn all() -> Vec<Face> {
    let mut v = Vec::new();
    v.extend(comp_fet::faces());
    v.extend(comp_tube::faces());
    v.extend(comp_vca::faces());
    v.extend(comp_fet::lane::faces());
    v.extend(comp_tube::lane_faces());
    v.extend(comp_vca::lane_faces());
    v.push(comp_sc::face());
    v.extend(eq::faces());
    v.extend(delay::faces());
    v.extend(reverb::faces());
    v.extend(saturate::faces());
    v.extend(modulation::faces());
    v.extend(suite_funk_rhythm::faces());
    v.extend(suite_boutique_clean::faces());
    v.extend(suite_studio_blues::faces());
    v.extend(suite_poly_clean::faces());
    v.extend(rig_blues::faces());
    v.extend(rig_faces::faces());
    v.extend(rig_time::faces());
    v.extend(rig_mod::faces());
    v.extend(rig_cells::faces());
    v.extend(suite_djent_forge::faces());
    v.extend(suite_modern_rhythm::faces());
    v.extend(suite_blackout_gain::faces());
    for f in &mut v {
        if f.plugin == "comp" && f.file != "00-sc-eq" {
            comp_sc::add_key(f);
            comp_look::apply(&mut f.scene, f.file);
        }
        if f.plugin == "saturate" {
            comp_look::apply_saturator(&mut f.scene, f.file);
        }
        let scene = std::mem::replace(&mut f.scene, document_proto::Scene::new("Surface"));
        f.scene = face::stable_ids(scene, &format!("{}/{}", f.plugin, f.file));
    }
    v
}

/// Write every face (`OUT/<plugin>/<file>.frame` and its sidecar) and each
/// plugin's catalog under `out`; returns the number of faces.
///
/// # Panics
/// On a write error.
pub fn write_all(out: &std::path::Path) -> usize {
    write_only(out, &[])
}

/// As [`write_all`], writing only the faces whose file starts with one of
/// `only` (every face when empty) — the catalogs and manifest as ever.
/// For designing one face without rewriting the rest.
pub fn write_only(out: &std::path::Path, only: &[String]) -> usize {
    let faces = all();
    let chosen = |file: &str| only.is_empty() || only.iter().any(|o| file.starts_with(o.as_str()));
    for f in faces.iter().filter(|f| chosen(f.file)) {
        let path = out.join(f.plugin).join(format!("{}.frame", f.file));
        document_io::save_surface(&path, &f.scene).unwrap_or_else(|e| panic!("write face: {e}"));
    }
    for (plugin, catalog) in catalogs(&faces) {
        let path = out.join(plugin).join(format!("{plugin}.catalog.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&catalog).expect("json")).expect("write catalog");
    }
    std::fs::write(out.join(rig_faces::PLUGIN).join("faces.json"), rig_faces::manifest()).expect("write faces.json");
    faces.iter().filter(|f| chosen(f.file)).count()
}
