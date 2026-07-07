use crate::FtsCompParams;
use comp_profiles::{all_profiles, map_control_value, Constraint};

pub(crate) struct EffectiveCompParams {
    pub(crate) threshold_db: f64,
    pub(crate) ratio: f64,
    pub(crate) attack_ms: f64,
    pub(crate) release_ms: f64,
    pub(crate) knee_db: f64,
    pub(crate) auto_makeup: bool,
    pub(crate) feedback: f64,
    pub(crate) channel_link: f64,
    pub(crate) detector_rms_mix: f64,
    pub(crate) drive: f64,
    pub(crate) character_mode: i32,
    pub(crate) input_gain_db: f64,
    pub(crate) output_gain_db: f64,
    pub(crate) range_db: f64,
    pub(crate) expander_threshold_db: f64,
    pub(crate) expander_ratio: f64,
    pub(crate) upward_threshold_db: f64,
    pub(crate) upward_ratio: f64,
    pub(crate) style: i32,
}

impl EffectiveCompParams {
    pub(crate) fn from_params(params: &FtsCompParams) -> Self {
        Self {
            threshold_db: params.threshold_db.value() as f64,
            ratio: params.ratio.value() as f64,
            attack_ms: params.attack_ms.value() as f64,
            release_ms: params.release_ms.value() as f64,
            knee_db: params.knee_db.value() as f64,
            auto_makeup: params.auto_makeup.value() > 0.5,
            feedback: params.feedback.value() as f64,
            channel_link: params.channel_link.value() as f64,
            detector_rms_mix: params.detector_rms_mix.value() as f64,
            drive: params.drive.value() as f64,
            character_mode: params.character_mode.value(),
            input_gain_db: params.input_gain_db.value() as f64,
            output_gain_db: params.output_gain_db.value() as f64,
            range_db: params.range_db.value() as f64,
            expander_threshold_db: params.expander_threshold_db.value() as f64,
            expander_ratio: params.expander_ratio.value() as f64,
            upward_threshold_db: params.upward_threshold_db.value() as f64,
            upward_ratio: params.upward_ratio.value() as f64,
            style: params.style.value(),
        }
    }

    pub(crate) fn apply_profile_macros(&mut self, profile_index: i32, drive: f64, output: f64) {
        for (control_id, normalized) in profile_macro_controls(profile_index, drive, output) {
            if let Some(writes) =
                map_control_value(profile_for_index(profile_index), control_id, normalized)
            {
                for (param, value) in writes {
                    self.apply_param_write(param, value);
                }
            }
        }
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &[Constraint]) {
        self.threshold_db = constrained_f64("threshold_db", self.threshold_db, constraints);
        self.ratio = constrained_f64("ratio", self.ratio, constraints);
        self.attack_ms = constrained_f64("attack_ms", self.attack_ms, constraints);
        self.release_ms = constrained_f64("release_ms", self.release_ms, constraints);
        self.knee_db = constrained_f64("knee_db", self.knee_db, constraints);
        self.feedback = constrained_f64("feedback", self.feedback, constraints);
        self.channel_link = constrained_f64("channel_link", self.channel_link, constraints);
        self.detector_rms_mix =
            constrained_f64("detector_rms_mix", self.detector_rms_mix, constraints);
        self.drive = constrained_f64("drive", self.drive, constraints);
        self.character_mode =
            constrained_f64("character_mode", self.character_mode as f64, constraints).round()
                as i32;
        self.input_gain_db = constrained_f64("input_gain_db", self.input_gain_db, constraints);
        self.output_gain_db = constrained_f64("output_gain_db", self.output_gain_db, constraints);
        self.range_db = constrained_f64("range_db", self.range_db, constraints);
        self.expander_threshold_db = constrained_f64(
            "expander_threshold_db",
            self.expander_threshold_db,
            constraints,
        );
        self.expander_ratio = constrained_f64("expander_ratio", self.expander_ratio, constraints);
        self.upward_threshold_db =
            constrained_f64("upward_threshold_db", self.upward_threshold_db, constraints);
        self.upward_ratio = constrained_f64("upward_ratio", self.upward_ratio, constraints);
        self.style = constrained_f64("style", self.style as f64, constraints).round() as i32;
    }

    fn apply_param_write(&mut self, param: &str, value: f64) {
        match param {
            "threshold_db" => self.threshold_db = value,
            "ratio" => self.ratio = value,
            "attack_ms" => self.attack_ms = value,
            "release_ms" => self.release_ms = value,
            "knee_db" => self.knee_db = value,
            "feedback" => self.feedback = value,
            "channel_link" => self.channel_link = value,
            "detector_rms_mix" => self.detector_rms_mix = value,
            "drive" => self.drive = value,
            "character_mode" => self.character_mode = value.round() as i32,
            "input_gain_db" => self.input_gain_db = value,
            "output_gain_db" => self.output_gain_db = value,
            "range_db" => self.range_db = value,
            "expander_threshold_db" => self.expander_threshold_db = value,
            "expander_ratio" => self.expander_ratio = value,
            "upward_threshold_db" => self.upward_threshold_db = value,
            "upward_ratio" => self.upward_ratio = value,
            "style" => self.style = value.round() as i32,
            _ => {}
        }
    }
}

pub(crate) fn profile_for_index(index: i32) -> &'static dyn comp_profiles::Profile {
    let profiles = all_profiles();
    profiles
        .get(index.clamp(0, (profiles.len() - 1) as i32) as usize)
        .copied()
        .unwrap_or(profiles[0])
}

pub(crate) fn profile_name(index: i32) -> &'static str {
    profile_for_index(index).name()
}

pub(crate) fn profile_macro_controls(
    profile_index: i32,
    drive: f64,
    output: f64,
) -> Vec<(&'static str, f64)> {
    match profile_for_index(profile_index).id() {
        "la2a" => vec![("peak_reduction", drive), ("gain", output)],
        "ssl_bus" => vec![("threshold", drive), ("makeup", output)],
        "urei_1176" => vec![("input", drive), ("output", output)],
        _ => Vec::new(),
    }
}

pub(crate) fn constrained_f64(param: &str, value: f64, constraints: &[Constraint]) -> f64 {
    let mut effective = value;
    for constraint in constraints {
        match constraint {
            Constraint::Fixed {
                param: constrained,
                value,
            } if *constrained == param => effective = *value,
            Constraint::Clamped {
                param: constrained,
                range,
            } if *constrained == param => {
                effective = effective.clamp(*range.start(), *range.end());
            }
            Constraint::SteppedOnly {
                param: constrained,
                values,
            } if *constrained == param && !values.is_empty() => {
                effective = values
                    .iter()
                    .copied()
                    .min_by(|a, b| {
                        (a - effective)
                            .abs()
                            .partial_cmp(&(b - effective).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap_or(effective);
            }
            _ => {}
        }
    }
    effective
}
