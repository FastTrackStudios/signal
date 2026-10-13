use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask error: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(task) = args.next() else {
        print_usage();
        return Ok(());
    };
    let passthrough: Vec<String> = args.collect();

    let workspace_root = workspace_root()?;

    match task.as_str() {
        "parity_test" | "partiy_test" => run_script(
            &workspace_root,
            workspace_root.join("xtask/frame/scripts/run_parity_render_and_diff.sh"),
            &passthrough,
        ),
        "parity_render" => run_script(
            &workspace_root,
            workspace_root.join("xtask/frame/scripts/render_parity_actuals.sh"),
            &passthrough,
        ),
        "parity_diff" => run_script(
            &workspace_root,
            workspace_root.join("xtask/frame/scripts/run_parity_scenarios.sh"),
            &passthrough,
        ),
        "parity_svg" => run_script(
            &workspace_root,
            workspace_root.join("xtask/frame/scripts/run_svg_parity_scenarios.sh"),
            &passthrough,
        ),
        "parity_svg_analyze" => run_script(
            &workspace_root,
            workspace_root.join("xtask/frame/scripts/analyze_svg_canonical_precision.sh"),
            &passthrough,
        ),
        "parity_svg_hash" => run_script(
            &workspace_root,
            workspace_root.join("xtask/frame/scripts/check_svg_hash_baselines.sh"),
            &passthrough,
        ),
        "svg_to_png" => cmd_svg_to_png(&passthrough),
        "parity_svg_png" => cmd_parity_svg_png(&workspace_root, &passthrough),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => Err(format!("unknown xtask command: {other}")),
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // xtask/frame → the repo root.
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "failed resolving workspace root".to_string())
}

fn run_script(root: &Path, script: PathBuf, args: &[String]) -> Result<(), String> {
    if !script.exists() {
        return Err(format!("script not found: {}", script.display()));
    }

    let status = Command::new(&script)
        .args(args)
        .current_dir(root)
        .env(
            "CARGO",
            env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()),
        )
        .status()
        .map_err(|e| format!("failed to run {}: {e}", script.display()))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} failed with status {}",
            script.display(),
            status
        ))
    }
}

// ---------------------------------------------------------------------------
// svg_to_png: rasterize an SVG to PNG via resvg
// ---------------------------------------------------------------------------

fn cmd_svg_to_png(args: &[String]) -> Result<(), String> {
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut width_override: Option<u32> = None;
    let mut height_override: Option<u32> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" | "-o" => {
                i += 1;
                output = Some(args.get(i).ok_or("--out requires a value")?.clone());
            }
            "--width" => {
                i += 1;
                width_override = Some(
                    args.get(i)
                        .ok_or("--width requires a value")?
                        .parse::<u32>()
                        .map_err(|e| format!("bad --width: {e}"))?,
                );
            }
            "--height" => {
                i += 1;
                height_override = Some(
                    args.get(i)
                        .ok_or("--height requires a value")?
                        .parse::<u32>()
                        .map_err(|e| format!("bad --height: {e}"))?,
                );
            }
            s if s.starts_with('-') => return Err(format!("unknown flag: {s}")),
            _ => {
                if input.is_some() {
                    return Err("multiple input files given".into());
                }
                input = Some(args[i].clone());
            }
        }
        i += 1;
    }

    let input = input.ok_or("usage: cargo xtask svg_to_png <input.svg> --out <output.png>")?;
    let output = output.ok_or("--out <path.png> is required")?;

    svg_to_png(
        Path::new(&input),
        Path::new(&output),
        width_override,
        height_override,
    )?;
    eprintln!("wrote {output}");
    Ok(())
}

fn svg_to_png(
    svg_path: &Path,
    png_path: &Path,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(), String> {
    let svg_data =
        std::fs::read(svg_path).map_err(|e| format!("read {}: {e}", svg_path.display()))?;

    let opt = resvg::usvg::Options::default();
    let tree =
        resvg::usvg::Tree::from_data(&svg_data, &opt).map_err(|e| format!("parse SVG: {e}"))?;

    let svg_size = tree.size();
    let (px_w, px_h) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            let scale = w as f32 / svg_size.width();
            (w, (svg_size.height() * scale).round() as u32)
        }
        (None, Some(h)) => {
            let scale = h as f32 / svg_size.height();
            ((svg_size.width() * scale).round() as u32, h)
        }
        (None, None) => (
            svg_size.width().round() as u32,
            svg_size.height().round() as u32,
        ),
    };

    if px_w == 0 || px_h == 0 {
        return Err(format!("degenerate size {px_w}x{px_h}"));
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(px_w, px_h)
        .ok_or_else(|| format!("failed to create {px_w}x{px_h} pixmap"))?;

    let scale_x = px_w as f32 / svg_size.width();
    let scale_y = px_h as f32 / svg_size.height();
    let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    pixmap
        .save_png(png_path)
        .map_err(|e| format!("save PNG: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// parity_svg_png: batch compare SVG→PNG rasterization vs wgpu render
// ---------------------------------------------------------------------------

fn cmd_parity_svg_png(workspace_root: &Path, args: &[String]) -> Result<(), String> {
    let scenarios_dir = workspace_root.join("crates/frame/render/tests/parity/scenarios");
    let core10_path = scenarios_dir.join("core10.txt");

    let mut scenario_filter: Option<String> = None;
    let mut run_all = false;
    let mut run_core10 = false;
    let mut threshold: f64 = 2.0;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--scenario" | "-s" => {
                i += 1;
                scenario_filter = Some(args.get(i).ok_or("--scenario requires a value")?.clone());
            }
            "--all" => run_all = true,
            "--core10" => run_core10 = true,
            "--threshold" | "-t" => {
                i += 1;
                threshold = args
                    .get(i)
                    .ok_or("--threshold requires a value")?
                    .parse::<f64>()
                    .map_err(|e| format!("bad --threshold: {e}"))?;
            }
            s if s.starts_with('-') => return Err(format!("unknown flag: {s}")),
            _ => {
                // bare arg treated as scenario name
                scenario_filter = Some(args[i].clone());
            }
        }
        i += 1;
    }

    let scenarios = resolve_scenarios(
        &scenarios_dir,
        &core10_path,
        scenario_filter.as_deref(),
        run_all,
        run_core10,
    )?;

    if scenarios.is_empty() {
        return Err("no scenarios matched".into());
    }

    let mut any_failure = false;

    for name in &scenarios {
        let dir = scenarios_dir.join(name);
        let actual_svg = dir.join("actual.svg");
        let actual_png = dir.join("actual.png");
        let expected_png = dir.join("expected.png");

        if !actual_svg.exists() {
            eprintln!("[skip] {name}: no actual.svg");
            continue;
        }

        // Rasterize SVG → actual-svg.png
        let svg_png = dir.join("actual-svg.png");
        if let Err(e) = svg_to_png(&actual_svg, &svg_png, None, None) {
            eprintln!("[error] {name}: svg_to_png failed: {e}");
            any_failure = true;
            continue;
        }

        eprintln!("── {name} ──");

        // Compare svg-rasterized vs wgpu render
        if actual_png.exists() {
            match pixel_diff(&svg_png, &actual_png, threshold) {
                Ok(result) => {
                    let label = if result.passed { "PASS" } else { "FAIL" };
                    eprintln!(
                        "  svg vs wgpu:   [{label}] {:.2}% diff ({} px), max_delta={}",
                        result.diff_percent, result.diff_pixels, result.max_channel_delta
                    );
                    println!("{}", result.to_json(name, "svg_vs_wgpu"));
                    if !result.passed {
                        any_failure = true;
                    }
                }
                Err(e) => {
                    eprintln!("  svg vs wgpu:   [ERROR] {e}");
                    any_failure = true;
                }
            }
        } else {
            eprintln!("  svg vs wgpu:   [skip] no actual.png");
        }

        // Compare svg-rasterized vs Figma reference
        if expected_png.exists() {
            match pixel_diff(&svg_png, &expected_png, threshold) {
                Ok(result) => {
                    let label = if result.passed { "PASS" } else { "FAIL" };
                    eprintln!(
                        "  svg vs figma:  [{label}] {:.2}% diff ({} px), max_delta={}",
                        result.diff_percent, result.diff_pixels, result.max_channel_delta
                    );
                    println!("{}", result.to_json(name, "svg_vs_figma"));
                    if !result.passed {
                        any_failure = true;
                    }
                }
                Err(e) => {
                    eprintln!("  svg vs figma:  [ERROR] {e}");
                    any_failure = true;
                }
            }
        } else {
            eprintln!("  svg vs figma:  [skip] no expected.png");
        }
    }

    if any_failure {
        Err("one or more comparisons failed or exceeded threshold".into())
    } else {
        Ok(())
    }
}

fn resolve_scenarios(
    scenarios_dir: &Path,
    core10_path: &Path,
    filter: Option<&str>,
    all: bool,
    _core10: bool,
) -> Result<Vec<String>, String> {
    if let Some(name) = filter {
        return Ok(vec![name.to_string()]);
    }

    if all {
        return list_scenario_dirs(scenarios_dir);
    }

    // Default: core10
    let text = std::fs::read_to_string(core10_path).map_err(|e| format!("read core10.txt: {e}"))?;
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect())
}

fn list_scenario_dirs(dir: &Path) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("readdir: {e}"))?;
        if entry.file_type().map_or(false, |t| t.is_dir()) {
            if let Some(name) = entry.file_name().to_str() {
                // skip hidden dirs and the manifest template
                if !name.starts_with('.') {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

// ---------------------------------------------------------------------------
// Pixel diff (same algorithm as visual_diff binary)
// ---------------------------------------------------------------------------

struct DiffResult {
    width: u32,
    height: u32,
    diff_pixels: u64,
    diff_percent: f64,
    mean_abs_delta: f64,
    max_channel_delta: u8,
    threshold_percent: f64,
    passed: bool,
}

impl DiffResult {
    fn to_json(&self, scenario: &str, comparison: &str) -> String {
        format!(
            "{{\"scenario\":\"{}\",\"comparison\":\"{}\",\"width\":{},\"height\":{},\"diffPixels\":{},\"diffPercent\":{:.6},\"meanAbsDeltaPerChannel\":{:.6},\"maxChannelDelta\":{},\"thresholdPercent\":{:.6},\"passed\":{}}}",
            scenario,
            comparison,
            self.width,
            self.height,
            self.diff_pixels,
            self.diff_percent,
            self.mean_abs_delta,
            self.max_channel_delta,
            self.threshold_percent,
            if self.passed { "true" } else { "false" }
        )
    }
}

fn pixel_diff(a_path: &Path, b_path: &Path, threshold_percent: f64) -> Result<DiffResult, String> {
    let a_img = image::open(a_path)
        .map_err(|e| format!("open {}: {e}", a_path.display()))?
        .to_rgba8();
    let b_img = image::open(b_path)
        .map_err(|e| format!("open {}: {e}", b_path.display()))?
        .to_rgba8();

    let (aw, ah) = a_img.dimensions();
    let (bw, bh) = b_img.dimensions();

    // If dimensions differ, resize a to match b (b is the reference)
    let (a_img, b_img, w, h) = if (aw, ah) != (bw, bh) {
        // Use the reference (b) dimensions
        let resized =
            image::imageops::resize(&a_img, bw, bh, image::imageops::FilterType::Lanczos3);
        (resized, b_img, bw, bh)
    } else {
        (a_img, b_img, aw, ah)
    };

    let total_px = (w as u64) * (h as u64);
    if total_px == 0 {
        return Err("empty images".into());
    }

    let mut diff_px: u64 = 0;
    let mut max_channel_delta: u8 = 0;
    let mut total_abs_delta: u64 = 0;

    for (a, b) in a_img.pixels().zip(b_img.pixels()) {
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
    let mean_abs_delta = total_abs_delta as f64 / (total_px as f64 * 4.0);
    let passed = diff_percent <= threshold_percent;

    Ok(DiffResult {
        width: w,
        height: h,
        diff_pixels: diff_px,
        diff_percent,
        mean_abs_delta,
        max_channel_delta,
        threshold_percent,
        passed,
    })
}

fn print_usage() {
    eprintln!(
        "usage: cargo xtask <command> [args...]

commands:
  parity_test   render actual PNGs + run parity diffs/reports
  partiy_test   typo alias for parity_test
  parity_render render actual PNGs only
  parity_diff   run parity diff/report only
  parity_svg    run SVG strict/canonical parity summary
  parity_svg_analyze analyze canonical diff sensitivity by precision
  parity_svg_hash verify approved SVG SHA-256 baselines
  svg_to_png    rasterize SVG to PNG via resvg
  parity_svg_png compare SVG-rasterized PNG vs wgpu render and Figma reference

examples:
  cargo xtask parity_test --all --threshold 0.0
  cargo xtask parity_test --scenario paint.gradient-matrix --threshold 0.0
  cargo xtask parity_render --all
  cargo xtask parity_diff --all --threshold 0.0
  cargo xtask parity_svg --all
  cargo xtask parity_svg_analyze --scenario text.mixed-style-runs
  cargo xtask parity_svg_hash
  cargo xtask svg_to_png input.svg --out output.png [--width 800] [--height 600]
  cargo xtask parity_svg_png --core10
  cargo xtask parity_svg_png --scenario regression.kitchen-sink --threshold 1.0
  cargo xtask parity_svg_png --all"
    );
}
