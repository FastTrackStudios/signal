#[cfg(feature = "anyrender")]
fn main() {
    if let Err(err) = run() {
        eprintln!("visual_diff error: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("visual_diff requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let actual = args.next().ok_or_else(|| {
        "usage: visual_diff <actual.png> <expected.png> [threshold_percent]".to_string()
    })?;
    let expected = args.next().ok_or_else(|| {
        "usage: visual_diff <actual.png> <expected.png> [threshold_percent]".to_string()
    })?;
    let threshold_percent = args
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(2.0)
        .max(0.0);

    let actual_img = image::open(&actual)
        .map_err(|e| format!("failed to open actual image {}: {}", actual, e))?
        .to_rgba8();
    let expected_img = image::open(&expected)
        .map_err(|e| format!("failed to open expected image {}: {}", expected, e))?
        .to_rgba8();

    if actual_img.dimensions() != expected_img.dimensions() {
        return Err(format!(
            "dimension mismatch: actual {:?}, expected {:?}",
            actual_img.dimensions(),
            expected_img.dimensions()
        ));
    }

    let (w, h) = actual_img.dimensions();
    let total_px = (w as u64) * (h as u64);
    if total_px == 0 {
        return Err("empty images are not supported".to_string());
    }

    let mut diff_px: u64 = 0;
    let mut max_channel_delta: u8 = 0;
    let mut total_abs_delta: u64 = 0;

    for (a, b) in actual_img.pixels().zip(expected_img.pixels()) {
        let mut pixel_diff = false;
        for c in 0..4 {
            let da = a[c].abs_diff(b[c]);
            if da != 0 {
                pixel_diff = true;
            }
            if da > max_channel_delta {
                max_channel_delta = da;
            }
            total_abs_delta += da as u64;
        }
        if pixel_diff {
            diff_px += 1;
        }
    }

    let diff_percent = (diff_px as f64 / total_px as f64) * 100.0;
    let mean_abs_delta_per_channel = total_abs_delta as f64 / (total_px as f64 * 4.0);
    let passed = diff_percent <= threshold_percent;

    println!(
        "{{\"actual\":\"{}\",\"expected\":\"{}\",\"width\":{},\"height\":{},\"diffPixels\":{},\"diffPercent\":{:.6},\"meanAbsDeltaPerChannel\":{:.6},\"maxChannelDelta\":{},\"thresholdPercent\":{:.6},\"passed\":{}}}",
        actual,
        expected,
        w,
        h,
        diff_px,
        diff_percent,
        mean_abs_delta_per_channel,
        max_channel_delta,
        threshold_percent,
        if passed { "true" } else { "false" }
    );

    if passed {
        Ok(())
    } else {
        Err(format!(
            "diff_percent {:.4}% exceeded threshold {:.4}%",
            diff_percent, threshold_percent
        ))
    }
}
