use render::{build_scene_layout_boxes, build_scene_primitives};
use serde_json::json;
use std::time::Instant;

#[derive(Default, Clone, Copy)]
struct PrimitivePassCounts {
    group_start: usize,
    group_end: usize,
    layer_start: usize,
    layer_end: usize,
    clip_start: usize,
    clip_end: usize,
    mask_def_start: usize,
    mask_def_end: usize,
    mask_apply_start: usize,
    mask_apply_end: usize,
    rect: usize,
    path: usize,
    text: usize,
}

#[derive(Clone)]
struct Sample {
    import_ms: f64,
    layout_ms: f64,
    primitive_ms: f64,
    scene_ms: Option<f64>,
    total_ms: f64,
    node_count: usize,
    primitive_count: usize,
    pass_counts: PrimitivePassCounts,
}

fn avg(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn count_passes(primitives: &[render::PaintPrimitive]) -> PrimitivePassCounts {
    let mut out = PrimitivePassCounts::default();
    for p in primitives {
        match p {
            render::PaintPrimitive::GroupStart { .. } => out.group_start += 1,
            render::PaintPrimitive::GroupEnd { .. } => out.group_end += 1,
            render::PaintPrimitive::LayerStart { .. } => out.layer_start += 1,
            render::PaintPrimitive::LayerEnd { .. } => out.layer_end += 1,
            render::PaintPrimitive::ClipStart { .. } => out.clip_start += 1,
            render::PaintPrimitive::ClipEnd { .. } => out.clip_end += 1,
            render::PaintPrimitive::MaskDefStart { .. } => out.mask_def_start += 1,
            render::PaintPrimitive::MaskDefEnd { .. } => out.mask_def_end += 1,
            render::PaintPrimitive::MaskApplyStart { .. } => out.mask_apply_start += 1,
            render::PaintPrimitive::MaskApplyEnd { .. } => out.mask_apply_end += 1,
            render::PaintPrimitive::Rect { .. } => out.rect += 1,
            render::PaintPrimitive::Path { .. } => out.path += 1,
            render::PaintPrimitive::Text { .. } => out.text += 1,
        }
    }
    out
}

fn main() {
    if let Err(err) = run() {
        eprintln!("load_profile error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or_else(|| {
        "usage: load_profile <fts_export_or_figma_json> [--iterations N]".to_string()
    })?;
    let mut iterations = 3usize;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => {
                let v = args
                    .next()
                    .ok_or_else(|| "--iterations requires a value".to_string())?;
                iterations = v
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --iterations value {v}: {e}"))?
                    .max(1);
            }
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    let read_start = Instant::now();
    let bytes = std::fs::read(&input).map_err(|e| format!("failed reading {input}: {e}"))?;
    let read_ms = read_start.elapsed().as_secs_f64() * 1000.0;

    let mut samples: Vec<Sample> = Vec::with_capacity(iterations);
    let mut root_fallback = String::new();
    for _ in 0..iterations {
        let run_start = Instant::now();

        let t = Instant::now();
        let doc = figma_import::import_bytes(&bytes)
            .map_err(|e| format!("import failed for {input}: {e}"))?;
        let import_ms = t.elapsed().as_secs_f64() * 1000.0;

        let root = doc.pages().first().copied().unwrap_or(doc.root);
        if root_fallback.is_empty() {
            root_fallback = doc
                .node(root)
                .and_then(|n| n.source.as_ref())
                .map(|s| s.id.clone())
                .unwrap_or_default();
        }

        let t = Instant::now();
        let _layout = build_scene_layout_boxes(&doc, root);
        let layout_ms = t.elapsed().as_secs_f64() * 1000.0;

        let t = Instant::now();
        let primitives = build_scene_primitives(&doc, root);
        let primitive_ms = t.elapsed().as_secs_f64() * 1000.0;

        let pass_counts = count_passes(&primitives);
        let node_count = doc.walk(root).len();

        #[cfg(feature = "anyrender")]
        let scene_ms = {
            let t = Instant::now();
            let mut scene = vello::Scene::new();
            render::paint_primitives_into_scene_with(
                &mut scene,
                &primitives,
                kurbo::Affine::IDENTITY,
                None,
            );
            Some(t.elapsed().as_secs_f64() * 1000.0)
        };
        #[cfg(not(feature = "anyrender"))]
        let scene_ms = None;

        samples.push(Sample {
            import_ms,
            layout_ms,
            primitive_ms,
            scene_ms,
            total_ms: run_start.elapsed().as_secs_f64() * 1000.0,
            node_count,
            primitive_count: primitives.len(),
            pass_counts,
        });
    }

    let import_values: Vec<f64> = samples.iter().map(|s| s.import_ms).collect();
    let layout_values: Vec<f64> = samples.iter().map(|s| s.layout_ms).collect();
    let primitive_values: Vec<f64> = samples.iter().map(|s| s.primitive_ms).collect();
    let scene_values: Vec<f64> = samples.iter().filter_map(|s| s.scene_ms).collect();
    let total_values: Vec<f64> = samples.iter().map(|s| s.total_ms).collect();

    let payload = json!({
        "input": input,
        "bytes": bytes.len(),
        "root": root_fallback,
        "iterations": iterations,
        "readMs": read_ms,
        "avg": {
            "importMs": avg(&import_values),
            "layoutMs": avg(&layout_values),
            "primitiveMs": avg(&primitive_values),
            "sceneMs": if scene_values.is_empty() { serde_json::Value::Null } else { json!(avg(&scene_values)) },
            "totalMs": avg(&total_values),
            "nodeCount": samples.last().map(|s| s.node_count).unwrap_or(0),
            "primitiveCount": samples.last().map(|s| s.primitive_count).unwrap_or(0),
            "passCounts": {
                "groupStart": samples.last().map(|s| s.pass_counts.group_start).unwrap_or(0),
                "groupEnd": samples.last().map(|s| s.pass_counts.group_end).unwrap_or(0),
                "layerStart": samples.last().map(|s| s.pass_counts.layer_start).unwrap_or(0),
                "layerEnd": samples.last().map(|s| s.pass_counts.layer_end).unwrap_or(0),
                "clipStart": samples.last().map(|s| s.pass_counts.clip_start).unwrap_or(0),
                "clipEnd": samples.last().map(|s| s.pass_counts.clip_end).unwrap_or(0),
                "maskDefStart": samples.last().map(|s| s.pass_counts.mask_def_start).unwrap_or(0),
                "maskDefEnd": samples.last().map(|s| s.pass_counts.mask_def_end).unwrap_or(0),
                "maskApplyStart": samples.last().map(|s| s.pass_counts.mask_apply_start).unwrap_or(0),
                "maskApplyEnd": samples.last().map(|s| s.pass_counts.mask_apply_end).unwrap_or(0),
                "rect": samples.last().map(|s| s.pass_counts.rect).unwrap_or(0),
                "path": samples.last().map(|s| s.pass_counts.path).unwrap_or(0),
                "text": samples.last().map(|s| s.pass_counts.text).unwrap_or(0),
            }
        },
        "samples": samples.iter().map(|s| json!({
            "importMs": s.import_ms,
            "layoutMs": s.layout_ms,
            "primitiveMs": s.primitive_ms,
            "sceneMs": s.scene_ms,
            "totalMs": s.total_ms,
            "nodeCount": s.node_count,
            "primitiveCount": s.primitive_count
        })).collect::<Vec<_>>()
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
    );
    Ok(())
}
