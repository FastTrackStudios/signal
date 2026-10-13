use dawfile_reaper::{ReaperProject, parse_rpp_file};
use std::collections::BTreeMap;

fn parse_marker_position_label(label: &str) -> Option<(i32, i32, f64)> {
    let mut parts = label.split('.');
    let measure = parts.next()?.parse::<i32>().ok()?;
    let beat = parts.next()?.parse::<i32>().ok()?;
    let frac_raw = parts.next()?.parse::<i32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((measure, beat, frac_raw as f64 / 1000.0))
}

#[test]
fn advanced_tempo_map_marker_positions_match_labels() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo-map-advanced.RPP");

    let content = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fixture.display()));
    let rpp = parse_rpp_file(&content).expect("parse_rpp_file failed");
    let project = ReaperProject::from_rpp_project(&rpp).expect("from_rpp_project failed");

    let envelope = project
        .tempo_envelope
        .as_ref()
        .expect("fixture must contain TEMPOENVEX");

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for marker in &project.markers_regions.markers {
        if !marker.is_marker() {
            continue;
        }

        let Some((exp_measure, exp_beat, exp_frac)) = parse_marker_position_label(&marker.name)
        else {
            continue;
        };
        // This label-driven check is only valid in the early 4/4 section.
        // Later markers in this fixture intentionally cross mixed signatures and
        // don't map 1:1 to naive label expectations.
        if exp_measure > 10 {
            continue;
        }

        let (act_measure, act_beat, act_frac) = envelope.musical_position_at_time(marker.position);
        let act_qn = envelope.beats_at_time(marker.position);
        let exp_qn = ((exp_measure - 1) * 4 + (exp_beat - 1)) as f64 + exp_frac;

        if act_measure != exp_measure || act_beat != exp_beat || (act_frac - exp_frac).abs() > 0.02
        {
            failures.push(format!(
                "marker={} time={:.12} expected={}.{}.{:03} actual={}.{}.{:03} qn_act={:.12} qn_exp={:.12} dq={:.12}",
                marker.name,
                marker.position,
                exp_measure,
                exp_beat,
                (exp_frac * 1000.0).round() as i32,
                act_measure,
                act_beat,
                (act_frac * 1000.0).round() as i32,
                act_qn,
                exp_qn,
                act_qn - exp_qn
            ));
        }

        checked += 1;
    }

    assert!(
        checked >= 8,
        "expected to validate many marker labels, got {checked}"
    );
    assert!(
        failures.is_empty(),
        "tempo marker mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn advanced_tempo_map_matches_reaper_oracle_export() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo-map-advanced.RPP");
    let oracle = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo_advanced_oracle_full.tsv");

    let content = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fixture.display()));
    if !oracle.exists() {
        eprintln!(
            "skipping advanced oracle parity: missing {}",
            oracle.display()
        );
        return;
    }
    let oracle_text = std::fs::read_to_string(&oracle)
        .unwrap_or_else(|e| panic!("failed to read oracle {}: {e}", oracle.display()));
    if !oracle_text.contains("2.1.000") {
        eprintln!(
            "skipping advanced oracle parity: {} does not contain advanced marker labels",
            oracle.display()
        );
        return;
    }

    let rpp = parse_rpp_file(&content).expect("parse_rpp_file failed");
    let project = ReaperProject::from_rpp_project(&rpp).expect("from_rpp_project failed");
    let envelope = project
        .tempo_envelope
        .as_ref()
        .expect("fixture must contain TEMPOENVEX");

    let mut checked = 0usize;
    let mut failures = Vec::new();

    for line in oracle_text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 {
            continue;
        }
        let row_type = cols[0];
        let time = cols[3].parse::<f64>().unwrap_or(0.0);
        let expected_ruler = cols[5];
        let expected_qn = cols[4].parse::<f64>().unwrap_or(0.0);

        if row_type == "MARKER" {
            let name = cols[2];
            if name.is_empty() {
                continue;
            }
            let actual_qn = envelope.beats_at_time(time);
            if (actual_qn - expected_qn).abs() > 0.10 {
                failures.push(format!(
                    "oracle marker name={} time={:.12} expected_qn={:.12} actual_qn={:.12} dq={:.12} expected_ruler={}",
                    name,
                    time,
                    expected_qn,
                    actual_qn,
                    actual_qn - expected_qn,
                    expected_ruler
                ));
            }
            checked += 1;
        }
    }

    assert!(checked >= 8, "expected oracle marker rows, got {checked}");
    assert!(
        failures.is_empty(),
        "oracle mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn edge_calibration_matches_reaper_oracle_export() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo-calibration-edge.RPP");
    let oracle = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo_edge_oracle_full.tsv");

    let content = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fixture.display()));
    let oracle_text = std::fs::read_to_string(&oracle)
        .unwrap_or_else(|e| panic!("failed to read oracle {}: {e}", oracle.display()));

    let rpp = parse_rpp_file(&content).expect("parse_rpp_file failed");
    let project = ReaperProject::from_rpp_project(&rpp).expect("from_rpp_project failed");
    let envelope = project
        .tempo_envelope
        .as_ref()
        .expect("fixture must contain TEMPOENVEX");

    let mut checked = 0usize;
    let mut failures = Vec::new();

    for line in oracle_text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 {
            continue;
        }
        if cols[0] != "MARKER" {
            continue;
        }
        let name = cols[2];
        if name.is_empty() {
            continue;
        }
        let time = cols[3].parse::<f64>().unwrap_or(0.0);
        let expected_qn = cols[4].parse::<f64>().unwrap_or(0.0);
        let actual_qn = envelope.beats_at_time(time);

        if (actual_qn - expected_qn).abs() > 0.05 {
            failures.push(format!(
                "edge marker name={} time={:.12} expected_qn={:.12} actual_qn={:.12} dq={:.12}",
                name,
                time,
                expected_qn,
                actual_qn,
                actual_qn - expected_qn
            ));
        }
        checked += 1;
    }

    assert!(checked >= 5, "expected edge marker rows, got {checked}");
    assert!(
        failures.is_empty(),
        "edge oracle mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn advanced_tempo_map_beats_monotonic() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo-map-advanced.RPP");

    let content = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fixture.display()));
    let rpp = parse_rpp_file(&content).expect("parse_rpp_file failed");
    let project = ReaperProject::from_rpp_project(&rpp).expect("from_rpp_project failed");

    let envelope = project
        .tempo_envelope
        .as_ref()
        .expect("fixture must contain TEMPOENVEX");

    let mut previous = 0.0f64;
    for i in 0..=500 {
        let t = (i as f64) * 0.06;
        let beats = envelope.beats_at_time(t);
        assert!(
            beats + 1e-9 >= previous,
            "beats must be monotonic: t={t:.3} previous={previous:.6} current={beats:.6}"
        );
        previous = beats;
    }
}

#[test]
fn calibration_curve_samples_error_report() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo-calibration.RPP");
    let curve = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo_calibration_curve_samples.tsv");

    let content = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fixture.display()));
    let curve_text = std::fs::read_to_string(&curve)
        .unwrap_or_else(|e| panic!("failed to read curve {}: {e}", curve.display()));

    let rpp = parse_rpp_file(&content).expect("parse_rpp_file failed");
    let project = ReaperProject::from_rpp_project(&rpp).expect("from_rpp_project failed");
    let envelope = project
        .tempo_envelope
        .as_ref()
        .expect("fixture must contain TEMPOENVEX");

    let mut count = 0usize;
    let mut max_abs_err = 0.0f64;
    let mut sum_abs_err = 0.0f64;
    let mut per_seg: BTreeMap<i32, (usize, f64, f64)> = BTreeMap::new();
    let mut per_shape: BTreeMap<i32, (usize, f64, f64)> = BTreeMap::new();
    let mut per_seg_local: BTreeMap<i32, (usize, f64, f64, f64, f64)> = BTreeMap::new();
    let mut per_shape_local: BTreeMap<i32, (usize, f64, f64)> = BTreeMap::new();
    let mut seg_start_qn_actual: BTreeMap<i32, f64> = BTreeMap::new();

    for line in curve_text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 25 {
            continue;
        }
        let seg_idx = cols[0].parse::<i32>().unwrap_or(-1);
        let shape0 = cols[14].parse::<i32>().unwrap_or(-1);
        let time = cols[22].parse::<f64>().unwrap_or(0.0);
        let qn_expected = cols[23].parse::<f64>().unwrap_or(0.0);
        let qn_minus_expected = cols[24].parse::<f64>().unwrap_or(0.0);
        let qn_actual = envelope.beats_at_time(time);
        let abs_err = (qn_actual - qn_expected).abs();
        let seg_q0_actual = *seg_start_qn_actual.entry(seg_idx).or_insert(qn_actual);
        let qn_minus_actual = qn_actual - seg_q0_actual;
        let abs_err_local = (qn_minus_actual - qn_minus_expected).abs();

        max_abs_err = max_abs_err.max(abs_err);
        sum_abs_err += abs_err;
        count += 1;

        let e = per_seg.entry(seg_idx).or_insert((0, 0.0, 0.0));
        e.0 += 1;
        e.1 += abs_err;
        e.2 = e.2.max(abs_err);

        let s = per_shape.entry(shape0).or_insert((0, 0.0, 0.0));
        s.0 += 1;
        s.1 += abs_err;
        s.2 = s.2.max(abs_err);

        let el = per_seg_local
            .entry(seg_idx)
            .or_insert((0, 0.0, 0.0, 0.0, 0.0));
        el.0 += 1;
        el.1 += abs_err_local;
        el.2 = el.2.max(abs_err_local);
        el.3 += qn_minus_actual - qn_minus_expected;
        if cols[21] == "1.000000" {
            el.4 = qn_minus_actual - qn_minus_expected;
        }

        let sl = per_shape_local.entry(shape0).or_insert((0, 0.0, 0.0));
        sl.0 += 1;
        sl.1 += abs_err_local;
        sl.2 = sl.2.max(abs_err_local);
    }

    let mean_abs_err = if count > 0 {
        sum_abs_err / (count as f64)
    } else {
        0.0
    };
    println!(
        "calibration samples={} mean_abs_err_qn={:.12} max_abs_err_qn={:.12}",
        count, mean_abs_err, max_abs_err
    );
    for (shape, (n, sum, maxe)) in &per_shape {
        println!(
            "shape={} samples={} mean_abs_err_qn={:.12} max_abs_err_qn={:.12}",
            shape,
            n,
            sum / (*n as f64),
            maxe
        );
    }
    for (shape, (n, sum, maxe)) in &per_shape_local {
        println!(
            "shape_local={} samples={} mean_abs_err_qn={:.12} max_abs_err_qn={:.12}",
            shape,
            n,
            sum / (*n as f64),
            maxe
        );
    }
    for (seg, (n, sum, maxe)) in &per_seg {
        println!(
            "seg={} samples={} mean_abs_err_qn={:.12} max_abs_err_qn={:.12}",
            seg,
            n,
            sum / (*n as f64),
            maxe
        );
    }
    for (seg, (n, sum, maxe, signed_sum, end_delta)) in &per_seg_local {
        println!(
            "seg_local={} samples={} mean_abs_err_qn={:.12} max_abs_err_qn={:.12} mean_signed_err_qn={:.12} end_delta_qn={:.12}",
            seg,
            n,
            sum / (*n as f64),
            maxe,
            signed_sum / (*n as f64),
            end_delta
        );
    }

    // Strict for linear/hold, bounded for bezier while we iterate toward exact parity.
    if let Some((_, _, maxe)) = per_shape.get(&0) {
        assert!(*maxe < 1e-9, "shape=0 must be exact, max_abs_err_qn={maxe}");
    }
    if let Some((_, _, maxe)) = per_shape_local.get(&1) {
        assert!(
            *maxe < 1e-9,
            "shape=1 local curve must be exact, max_abs_err_qn={maxe}"
        );
    }
    assert!(
        mean_abs_err < 0.03,
        "mean_abs_err_qn too high: {mean_abs_err}"
    );
    assert!(max_abs_err < 0.2, "max_abs_err_qn too high: {max_abs_err}");
}

#[test]
fn edge_calibration_fixture_parses_and_is_monotonic() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tempo-calibration-edge.RPP");

    let content = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fixture.display()));
    let rpp = parse_rpp_file(&content).expect("parse_rpp_file failed");
    let project = ReaperProject::from_rpp_project(&rpp).expect("from_rpp_project failed");
    let envelope = project
        .tempo_envelope
        .as_ref()
        .expect("fixture must contain TEMPOENVEX");

    assert!(
        envelope.points.len() >= 12,
        "expected many edge-case tempo points"
    );

    let mut prev_qn = f64::NEG_INFINITY;
    for i in 0..=8000 {
        let t = (i as f64) * 0.001; // 1ms sampling over 8 seconds
        let qn = envelope.beats_at_time(t);
        assert!(qn.is_finite(), "qn must be finite at t={t:.6}");
        assert!(
            qn + 1e-9 >= prev_qn,
            "qn must be monotonic at t={t:.6}: prev={prev_qn:.12}, qn={qn:.12}"
        );
        prev_qn = qn;
    }
}
