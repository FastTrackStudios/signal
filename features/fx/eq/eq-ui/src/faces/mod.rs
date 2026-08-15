//! One front panel per EQ model.
//!
//! Selecting a model in the shell rail swaps the whole surface: `Main` is the
//! FTS curve editor, and the other four are the unit's own front panel, drawn
//! from the shared hardware kit in [`fts_audio_ui::hardware`].
//!
//! The EQ needs no mapping layer to do it. Unlike the compressor — where a
//! hardware control writes several engine params through `comp-profiles` — the
//! EQ's models already have their own parameters (`pultec_*`, `neve_*`,
//! `api_*`, `ssl_*`), because the DSP for each model is its own circuit rather
//! than a re-skin of one. So a panel control is one parameter, and
//! [`params_map`] is the whole translation.

pub mod params_map;
pub mod rack;
pub mod units;

use fts_audio_ui::shell::ShellItem;

/// A rail entry: an EQ family, and the models it cycles through.
pub struct EqCategory {
    pub id: &'static str,
    pub label: &'static str,
    pub badge: &'static str,
    /// `model` parameter values, in cycling order.
    pub models: &'static [i32],
    /// Badge per model, parallel to `models`.
    pub badges: &'static [&'static str],
}

/// The rail. `model` values are the ones `FtsEqParams::model` already uses —
/// 0 default, 1 Pultec, 2 Neve, 3 API, 4 SSL E, 5 SSL G — so the rail is a new
/// way to reach existing state, not new state.
pub static CATEGORIES: &[EqCategory] = &[
    EqCategory {
        id: "main",
        label: "Main — the FTS curve editor",
        badge: "MAIN",
        models: &[0],
        badges: &["MAIN"],
    },
    EqCategory {
        id: "pultec",
        label: "Pultec EQP-1A — passive program equalizer",
        badge: "PUL",
        models: &[1],
        badges: &["PUL"],
    },
    EqCategory {
        id: "ssl",
        label: "SSL channel EQ — E · G (click again to cycle)",
        badge: "SSL",
        models: &[4, 5],
        badges: &["E", "G"],
    },
    EqCategory {
        id: "api",
        label: "API 550A — three-band proportional Q",
        badge: "API",
        models: &[3],
        badges: &["550"],
    },
    EqCategory {
        id: "neve",
        label: "Neve 1073 — console channel EQ",
        badge: "73",
        models: &[2],
        badges: &["73"],
    },
];

/// The panel for a `model` value, if it has one — re-exported so the editor
/// does not have to know that designs live in [`units`].
pub use units::design_for as design_for_model;

/// The stable id for a `model` value — what a session persists instead of the
/// number. See `FtsEqParams::model_id`.
pub fn model_id(model: i32) -> &'static str {
    match model {
        1 => "pultec",
        2 => "neve_1073",
        3 => "api_550a",
        4 => "ssl_e",
        5 => "ssl_g",
        _ => "parametric",
    }
}

/// The `model` value an id names, if this build knows it.
pub fn model_for_id(id: &str) -> Option<i32> {
    (0..=5).find(|m| model_id(*m) == id)
}

/// The model a loaded session should be showing: the persisted id when it
/// names a model we still have, the index otherwise (sessions saved before
/// ids, or a project from a newer build).
pub fn resolved_model(params: &crate::params::FtsEqParams) -> i32 {
    let id = params.model_id.read();
    model_for_id(&id).unwrap_or_else(|| params.model.value())
}

/// The editor form a loaded session should open at. An unknown or missing id
/// means Responsive, which is the size the face asks for anyway.
pub fn resolved_form(params: &crate::params::FtsEqParams) -> fts_audio_ui::EditorForm {
    fts_audio_ui::EditorForm::from_id(&params.editor_form.read()).unwrap_or_default()
}

pub fn store_form(params: &crate::params::FtsEqParams, form: fts_audio_ui::EditorForm) {
    *params.editor_form.write() = form.id().to_string();
}

/// The editor size for a model *and* a chosen form: the form decides, except
/// for Responsive, which defers to the face.
pub fn editor_size_for(model: i32, form: fts_audio_ui::EditorForm) -> (u32, u32) {
    form.editor_size(fts_audio_ui::shell::RAIL_W, preferred_editor_size(model))
}

/// Record the id for `model` — call this wherever the model changes.
pub fn store_model_id(params: &crate::params::FtsEqParams, model: i32) {
    *params.model_id.write() = model_id(model).to_string();
}

/// The category a `model` value belongs to, and its position within it.
pub fn category_of(model: i32) -> Option<(usize, usize)> {
    CATEGORIES.iter().enumerate().find_map(|(ci, category)| {
        category
            .models
            .iter()
            .position(|m| *m == model)
            .map(|vi| (ci, vi))
    })
}

/// The rail, badged with the active model when its family is the active one.
pub fn rail_items(model: i32) -> Vec<ShellItem> {
    let active = category_of(model);
    CATEGORIES
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let badge = match active {
                Some((ci, vi)) if ci == index => {
                    category.badges.get(vi).copied().unwrap_or(category.badge)
                }
                _ => category.badge,
            };
            ShellItem::new(category.id, category.label).with_badge(badge)
        })
        .collect()
}

/// The `model` value a rail click selects: clicking the active family cycles
/// the models inside it, clicking another lands on its first.
pub fn rail_click_target(model: i32, clicked_category: usize) -> i32 {
    let Some(category) = CATEGORIES.get(clicked_category) else {
        return model;
    };
    match category_of(model) {
        Some((current, variant)) if current == clicked_category => {
            category.models[(variant + 1) % category.models.len()]
        }
        _ => category.models[0],
    }
}

/// The editor size a model wants, in logical px, including the shell rail.
///
/// The curve editor wants height; a rack unit is a wide, short drawing and
/// given a tall window just paints black above and below itself.
pub fn preferred_editor_size(model: i32) -> (u32, u32) {
    match units::design_for(model) {
        Some(design) => (
            (design.w + fts_audio_ui::shell::RAIL_W + 52.0) as u32,
            (design.h + 48.0) as u32,
        ),
        None => (crate::control_view::EDITOR_W, crate::control_view::EDITOR_H),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_the_parameter_can_hold_is_on_the_rail() {
        // `model` is an IntParam 0..=5; a value with no rail entry would be
        // reachable by automation and unreachable by clicking.
        for model in 0..=5 {
            assert!(
                category_of(model).is_some(),
                "model {model} is in no rail category"
            );
        }
    }

    #[test]
    fn clicking_the_active_family_cycles_and_wraps() {
        let ssl = CATEGORIES.iter().position(|c| c.id == "ssl").unwrap();
        assert_eq!(rail_click_target(4, ssl), 5, "E should advance to G");
        assert_eq!(rail_click_target(5, ssl), 4, "G should wrap back to E");
        // …and reaching the family from elsewhere lands on its first model.
        assert_eq!(rail_click_target(0, ssl), 4);
    }

    #[test]
    fn clicking_another_family_does_not_cycle_it() {
        let pultec = CATEGORIES.iter().position(|c| c.id == "pultec").unwrap();
        assert_eq!(rail_click_target(1, pultec), 1, "a lone model has no next");
        assert_eq!(rail_click_target(5, pultec), 1);
    }

    #[test]
    fn the_rail_badges_the_active_variant() {
        let ssl = CATEGORIES.iter().position(|c| c.id == "ssl").unwrap();
        assert_eq!(rail_items(4)[ssl].badge, "E");
        assert_eq!(rail_items(5)[ssl].badge, "G");
        // Inactive families wear the family badge.
        assert_eq!(rail_items(0)[ssl].badge, "SSL");
    }

    #[test]
    fn every_model_has_a_stable_id_and_round_trips_through_it() {
        // Ids are what a session restores from, so each must be distinct and
        // resolve back to the same model.
        let mut seen = Vec::new();
        for model in 0..=5 {
            let id = model_id(model);
            assert!(!seen.contains(&id), "duplicate model id {id}");
            seen.push(id);
            assert_eq!(model_for_id(id), Some(model));
        }
        // An id from a newer build resolves to nothing rather than to the
        // wrong model.
        assert_eq!(model_for_id("some_future_eq"), None);
    }

    #[test]
    fn hardware_models_ask_for_a_shorter_editor_than_the_curve() {
        let (_, curve_h) = preferred_editor_size(0);
        for model in 1..=5 {
            let (_, h) = preferred_editor_size(model);
            assert!(h < curve_h, "model {model} asks for {h}px, not shorter");
        }
    }
}
