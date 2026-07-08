//! Legacy SVG path/grid helpers for the EQ graph.
//!
//! The runtime graph is rendered through the vello painter, but these helpers
//! are kept isolated for tests and as a reference for any future SVG fallback.

#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use super::eq_graph_model::EqBand;
use super::eq_graph_response::{calculate_band_response, calculate_combined_response};

/// All EQ curve paths (combined and per-band).
#[derive(Clone, Default, PartialEq)]
pub struct AllEqCurves {
    /// Combined curve stroke path.
    pub combined_stroke: String,
    /// Combined curve fill path.
    pub combined_fill: String,
    /// Per-band curves: Vec of (band_index, stroke_path, fill_path) for each active band.
    pub band_curves: Vec<(usize, String, String)>,
}

#[allow(clippy::too_many_arguments)]
pub fn generate_all_eq_curves(
    bands: &[EqBand],
    sample_rate: f64,
    min_freq: f64,
    max_freq: f64,
    db_range: f64,
    padding: f64,
    graph_width: f64,
    graph_height: f64,
    num_points: usize,
) -> AllEqCurves {
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();

    let frequencies: Vec<f64> = (0..num_points)
        .map(|i| {
            let t = i as f64 / (num_points - 1) as f64;
            10.0_f64.powf(log_min + t * (log_max - log_min))
        })
        .collect();

    let freq_to_x = |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let db_to_y = |db: f64| -> f64 {
        let clamped = db.clamp(-db_range, db_range);
        let normalized = 0.5 - clamped / (2.0 * db_range);
        padding + normalized * graph_height
    };

    let zero_y = db_to_y(0.0);

    let combined_response: Vec<f64> = frequencies
        .iter()
        .map(|&freq| calculate_combined_response(bands, freq, sample_rate))
        .collect();

    let (combined_stroke, combined_fill) =
        build_curve_paths(&frequencies, &combined_response, freq_to_x, db_to_y, zero_y);

    let mut band_curves = Vec::new();
    for (idx, band) in bands.iter().enumerate() {
        if !band.used || !band.enabled {
            continue;
        }

        let band_response: Vec<f64> = frequencies
            .iter()
            .map(|&freq| calculate_band_response(band, freq, sample_rate))
            .collect();

        let (stroke, fill) =
            build_curve_paths(&frequencies, &band_response, freq_to_x, db_to_y, zero_y);
        band_curves.push((idx, stroke, fill));
    }

    AllEqCurves {
        combined_stroke,
        combined_fill,
        band_curves,
    }
}

fn build_curve_paths<F, G>(
    frequencies: &[f64],
    response_db: &[f64],
    freq_to_x: F,
    db_to_y: G,
    zero_y: f64,
) -> (String, String)
where
    F: Fn(f64) -> f64,
    G: Fn(f64) -> f64,
{
    let mut stroke_path = String::new();
    for (i, (&freq, &db)) in frequencies.iter().zip(response_db.iter()).enumerate() {
        let x = freq_to_x(freq);
        let y = db_to_y(db);
        if i == 0 {
            stroke_path.push_str(&format!("M{x:.2} {y:.2}"));
        } else {
            stroke_path.push_str(&format!("L{x:.2} {y:.2}"));
        }
    }

    let mut fill_path = String::new();
    let first_x = freq_to_x(frequencies[0]);
    fill_path.push_str(&format!("M{first_x:.2} {zero_y:.2}"));

    for (&freq, &db) in frequencies.iter().zip(response_db.iter()) {
        let x = freq_to_x(freq);
        let y = db_to_y(db);
        fill_path.push_str(&format!("L{x:.2} {y:.2}"));
    }

    let last_x = freq_to_x(*frequencies.last().unwrap());
    fill_path.push_str(&format!("L{last_x:.2} {zero_y:.2}Z"));

    (stroke_path, fill_path)
}

/// Generate the SVG path for the EQ curve.
///
/// Returns (stroke_path, fill_path)
#[allow(clippy::too_many_arguments)]
pub fn generate_eq_curve_path(
    bands: &[EqBand],
    sample_rate: f64,
    min_freq: f64,
    max_freq: f64,
    db_range: f64,
    padding: f64,
    graph_width: f64,
    graph_height: f64,
    num_points: usize,
) -> (String, String) {
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();

    let frequencies: Vec<f64> = (0..num_points)
        .map(|i| {
            let t = i as f64 / (num_points - 1) as f64;
            10.0_f64.powf(log_min + t * (log_max - log_min))
        })
        .collect();

    let response_db: Vec<f64> = frequencies
        .iter()
        .map(|&freq| calculate_combined_response(bands, freq, sample_rate))
        .collect();

    let freq_to_x = |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let db_to_y = |db: f64| -> f64 {
        let clamped = db.clamp(-db_range, db_range);
        let normalized = 0.5 - clamped / (2.0 * db_range);
        padding + normalized * graph_height
    };

    let zero_y = db_to_y(0.0);
    build_curve_paths(&frequencies, &response_db, freq_to_x, db_to_y, zero_y)
}

pub fn generate_grid_elements(
    padding: f64,
    graph_width: f64,
    graph_height: f64,
    min_freq: f64,
    max_freq: f64,
    db_range: f64,
) -> Vec<(f64, f64, f64, f64, bool)> {
    let mut lines = Vec::new();
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();

    let freq_to_x = |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let db_to_y = |db: f64| -> f64 {
        let normalized = 0.5 - db / (2.0 * db_range);
        padding + normalized * graph_height
    };

    let major_freqs = [100.0, 1000.0, 10000.0];
    let minor_freqs = [20.0, 50.0, 200.0, 500.0, 2000.0, 5000.0, 20000.0];

    for freq in major_freqs {
        if freq >= min_freq && freq <= max_freq {
            let x = freq_to_x(freq);
            lines.push((x, padding, x, padding + graph_height, true));
        }
    }

    for freq in minor_freqs {
        if freq >= min_freq && freq <= max_freq {
            let x = freq_to_x(freq);
            lines.push((x, padding, x, padding + graph_height, false));
        }
    }

    let y_zero = db_to_y(0.0);
    lines.push((padding, y_zero, padding + graph_width, y_zero, true));

    let db_step = 6.0;
    let mut db = db_step;
    while db <= db_range {
        let y_pos = db_to_y(db);
        let y_neg = db_to_y(-db);
        lines.push((padding, y_pos, padding + graph_width, y_pos, false));
        lines.push((padding, y_neg, padding + graph_width, y_neg, false));
        db += db_step;
    }

    lines
}

pub fn generate_freq_labels(
    padding: f64,
    graph_width: f64,
    height: f64,
    min_freq: f64,
    max_freq: f64,
) -> Vec<(f64, f64, String)> {
    let mut labels = Vec::new();
    let log_min = min_freq.log10();
    let log_max = max_freq.log10();
    let y = height - padding + 15.0;

    let freq_to_x = |freq: f64| -> f64 {
        let normalized = (freq.log10() - log_min) / (log_max - log_min);
        padding + normalized * graph_width
    };

    let freq_labels = [
        (20.0, "20"),
        (50.0, "50"),
        (100.0, "100"),
        (200.0, "200"),
        (500.0, "500"),
        (1000.0, "1k"),
        (2000.0, "2k"),
        (5000.0, "5k"),
        (10000.0, "10k"),
        (20000.0, "20k"),
    ];

    for (freq, label) in freq_labels {
        if freq >= min_freq && freq <= max_freq {
            labels.push((freq_to_x(freq), y, label.to_string()));
        }
    }

    labels
}

pub fn generate_db_labels(padding: f64, graph_height: f64, db_range: f64) -> Vec<(f64, f64, String)> {
    let mut labels = Vec::new();
    let x = padding - 10.0;

    let db_to_y = |db: f64| -> f64 {
        let normalized = 0.5 - db / (2.0 * db_range);
        padding + normalized * graph_height
    };

    labels.push((x, db_to_y(0.0), "0".to_string()));

    let db_step = 6.0;
    let mut db = db_step;
    while db <= db_range {
        labels.push((x, db_to_y(db), format!("+{}", db as i32)));
        labels.push((x, db_to_y(-db), format!("{}", -(db as i32))));
        db += db_step;
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::super::eq_graph_model::EqBandShape;
    use super::*;

    #[test]
    fn test_eq_band_default() {
        let band = EqBand::default();
        assert!(!band.used);
        assert!(!band.enabled);
        assert_eq!(band.gain, 0.0);
    }

    #[test]
    fn test_grid_generation() {
        let grid = generate_grid_elements(40.0, 720.0, 220.0, 20.0, 20000.0, 24.0);
        assert!(!grid.is_empty());

        let major_count = grid.iter().filter(|l| l.4).count();
        let minor_count = grid.iter().filter(|l| !l.4).count();
        assert!(major_count > 0);
        assert!(minor_count > 0);
    }

    #[test]
    fn test_freq_labels_generation() {
        let labels = generate_freq_labels(40.0, 720.0, 300.0, 20.0, 20000.0);
        assert!(!labels.is_empty());

        let label_texts: Vec<&str> = labels.iter().map(|l| l.2.as_str()).collect();
        assert!(label_texts.contains(&"100"));
        assert!(label_texts.contains(&"1k"));
        assert!(label_texts.contains(&"10k"));
    }

    #[test]
    fn test_db_labels_generation() {
        let labels = generate_db_labels(40.0, 220.0, 24.0);
        assert!(!labels.is_empty());

        let label_texts: Vec<&str> = labels.iter().map(|l| l.2.as_str()).collect();
        assert!(label_texts.contains(&"0"));
        assert!(label_texts.contains(&"+6"));
        assert!(label_texts.contains(&"-6"));
    }

    #[test]
    fn svg_curve_generation_returns_stroke_and_fill_paths() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 1000.0,
            gain: 6.0,
            q: 1.0,
            shape: EqBandShape::Bell,
            ..Default::default()
        };
        let curves =
            generate_all_eq_curves(&[band], 48000.0, 20.0, 20000.0, 24.0, 0.0, 800.0, 350.0, 64);
        assert!(curves.combined_stroke.starts_with('M'));
        assert!(curves.combined_fill.ends_with('Z'));
        assert_eq!(curves.band_curves.len(), 1);
    }
}
