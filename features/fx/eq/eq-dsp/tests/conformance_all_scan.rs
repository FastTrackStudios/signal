//! Data-driven conformance scanner across every filter type × every slope.
//!
//! Reports per-(filter, slope) pass / total + max error.  Useful for
//! tracking algorithmic conformance progress (run with FTSEQ_BYPASS_LOOKUP
//! to bypass per-filter lookup tables) and for spot-checking lookup
//! coverage.

use eq_dsp::design::{design_filter, FilterType};
use eq_dsp::response::compute_magnitude_response;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const TOL_DB: f64 = 0.005;
const SR: f64 = 48000.0;
// Skip cells where reference magnitude is below the audio noise floor.
// Below ~-100 dB, biquad-cascade evaluation accumulates cancellation noise
// that exceeds 0.005 dB even with bit-exact coefficients (sub-audible).
const SKIP_BELOW_DB: f64 = -100.0;

fn slope_to_order(slope: usize) -> usize {
    match slope {
        0 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 => 6,
        7 => 7,
        8 => 12,
        9 => 16,
        _ => slope,
    }
}

const FILTERS: &[(&str, FilterType, bool)] = &[
    ("bell", FilterType::Peak, true),
    ("high_shelf", FilterType::HighShelf, true),
    ("low_shelf", FilterType::LowShelf, true),
    ("tilt_shelf", FilterType::TiltShelf, true),
    ("flat_tilt", FilterType::FlatTilt, true),
    ("allpass", FilterType::Allpass, false),
    ("high_cut", FilterType::Highpass, false),
    ("low_cut", FilterType::Lowpass, false),
    ("notch", FilterType::Notch, false),
    ("bandpass", FilterType::Bandpass, false),
];

fn parse_filename(stem: &str, prefix: &str, has_gain: bool) -> Option<(f64, f64, f64, usize)> {
    let s = stem.strip_prefix(&format!("{}_", prefix))?;
    let (fc_part, rest) = s.split_once("hz_")?;
    let fc: f64 = fc_part.parse().ok()?;
    if has_gain {
        let (gain_part, rest) = rest.split_once("db_q")?;
        let gain: f64 = gain_part.parse().ok()?;
        let (q_part, slope_part) = rest.split_once("_s")?;
        let q: f64 = q_part.parse().ok()?;
        let slope: usize = slope_part.parse().ok()?;
        Some((fc, gain, q, slope))
    } else {
        let rest = rest.strip_prefix('q')?;
        let (q_part, slope_part) = rest.split_once("_s")?;
        let q: f64 = q_part.parse().ok()?;
        let slope: usize = slope_part.parse().ok()?;
        Some((fc, 0.0, q, slope))
    }
}

fn load_ref(path: &Path) -> Option<(Vec<f64>, Vec<f64>)> {
    let text = fs::read_to_string(path).ok()?;
    let mut freqs = Vec::new();
    let mut mags = Vec::new();
    for line in text.lines().skip(1) {
        let parts: Vec<_> = line.split(',').collect();
        if parts.len() < 2 {
            continue;
        }
        let f: f64 = parts[0].trim().parse().ok()?;
        let m: f64 = parts[1].trim().parse().ok()?;
        freqs.push(f);
        mags.push(m);
    }
    if mags.is_empty() {
        return None;
    }
    Some((freqs, mags))
}

#[test]
fn all_filters_all_slopes_scan() {
    let ref_dir = Path::new("tests/reference");
    let mut by: BTreeMap<(String, usize), (usize, usize, f64)> = BTreeMap::new();

    for entry in fs::read_dir(ref_dir).expect("ref dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        for (prefix, ftype, has_gain) in FILTERS {
            if !stem.starts_with(&format!("{}_", prefix)) {
                continue;
            }
            let (fc, gain, q, slope) = match parse_filename(&stem, prefix, *has_gain) {
                Some(t) => t,
                None => break,
            };
            let order = slope_to_order(slope);
            let (freqs, ref_mags) = match load_ref(&path) {
                Some(t) => t,
                None => break,
            };
            let sos = design_filter(*ftype, fc, q, gain, SR, order);
            if sos.is_empty() {
                break;
            }
            let our = compute_magnitude_response(&sos, &freqs, SR);
            let mut max_err = 0.0_f64;
            let mut pass = true;
            for i in 0..freqs.len() {
                if ref_mags[i] < SKIP_BELOW_DB {
                    continue;
                }
                if !our[i].is_finite() {
                    pass = false;
                    break;
                }
                let d = (our[i] - ref_mags[i]).abs();
                if d > max_err {
                    max_err = d;
                }
                if d > TOL_DB {
                    pass = false;
                }
            }
            let entry = by.entry((prefix.to_string(), slope)).or_insert((0, 0, 0.0));
            entry.1 += 1;
            if pass {
                entry.0 += 1;
            }
            if max_err > entry.2 {
                entry.2 = max_err;
            }
            break;
        }
    }

    println!("\nFilter conformance per slope (TOL_DB={}):", TOL_DB);
    println!(
        "  {:>12}  {:>5}  {:>5}  {:>5}  {:>10}",
        "filter", "slope", "pass", "total", "max_err"
    );
    let mut last_filter = String::new();
    let mut filter_pass = 0;
    let mut filter_total = 0;
    let mut grand_pass = 0;
    let mut grand_total = 0;
    for ((prefix, slope), (pass, total, max_err)) in &by {
        if prefix != &last_filter && !last_filter.is_empty() {
            println!(
                "  {:>12}  {:>5}  {:>5}  {:>5}",
                last_filter, "TOTAL", filter_pass, filter_total
            );
            filter_pass = 0;
            filter_total = 0;
        }
        last_filter = prefix.clone();
        println!(
            "  {:>12}  {:>5}  {:>5}  {:>5}  {:>10.4}",
            prefix, slope, pass, total, max_err
        );
        filter_pass += pass;
        filter_total += total;
        grand_pass += pass;
        grand_total += total;
    }
    println!(
        "  {:>12}  {:>5}  {:>5}  {:>5}",
        last_filter, "TOTAL", filter_pass, filter_total
    );
    println!(
        "\n  GRAND TOTAL: {} / {} ({:.1}%)",
        grand_pass,
        grand_total,
        100.0 * grand_pass as f64 / grand_total as f64
    );
}
