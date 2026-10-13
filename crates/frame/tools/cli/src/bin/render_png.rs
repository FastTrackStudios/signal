#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("render_png requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(err) = run() {
        eprintln!("render_png error: {err}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    use frame::render::build_scene_layout_boxes;
    use image::RgbaImage;
    use kurbo::Affine;
    use peniko::Color;
    use vello::Scene;

    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or_else(|| "usage: render_png <input_json> --out <output_png> [--root FIGMA_ID] [--width PX] [--height PX]".to_string())?;

    let mut out_path: Option<String> = None;
    let mut root_figma_id: Option<String> = None;
    let mut width_override: Option<u32> = None;
    let mut height_override: Option<u32> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out_path = args.next(),
            "--root" => root_figma_id = args.next(),
            "--width" => {
                width_override = Some(
                    args.next()
                        .ok_or_else(|| "--width requires value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --width value: {e}"))?,
                )
            }
            "--height" => {
                height_override = Some(
                    args.next()
                        .ok_or_else(|| "--height requires value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --height value: {e}"))?,
                )
            }
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    let out_path = out_path.ok_or_else(|| "--out <output_png> is required".to_string())?;
    let bytes = std::fs::read(&input).map_err(|e| format!("failed reading {input}: {e}"))?;
    let export_bbox = extract_export_bbox(&bytes, root_figma_id.as_deref())?;
    let base_color =
        extract_export_base_color(&bytes, root_figma_id.as_deref())?.unwrap_or(Color::WHITE);
    let doc = frame::figma::import::import_bytes(&bytes)
        .map_err(|e| format!("failed importing source document from {input}: {e}"))?;
    let root = match root_figma_id {
        Some(figma_id) => frame::figma::import::node_for_figma_id(&doc, &figma_id)
            .ok_or_else(|| format!("root figma id not found: {figma_id}"))?,
        None => doc.pages().first().copied().unwrap_or(doc.root),
    };

    let layout_boxes = build_scene_layout_boxes(&doc, root);
    let subtree = doc.walk(root);

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for node_id in subtree {
        if let Some(b) = layout_boxes.get(&node_id).copied() {
            if b.width <= 0.0 || b.height <= 0.0 {
                continue;
            }
            min_x = min_x.min(b.x);
            min_y = min_y.min(b.y);
            max_x = max_x.max(b.x + b.width);
            max_y = max_y.max(b.y + b.height);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        if let Some((x, y, w, h)) = export_bbox {
            min_x = x;
            min_y = y;
            max_x = x + w;
            max_y = y + h;
        } else {
            min_x = 0.0;
            min_y = 0.0;
            max_x = 1.0;
            max_y = 1.0;
        }
    }

    let (bbox_width, bbox_height) = export_bbox
        .map(|(_, _, w, h)| (w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32))
        .unwrap_or((1, 1));
    let natural_width = (max_x - min_x).ceil().max(1.0) as u32;
    let natural_height = (max_y - min_y).ceil().max(1.0) as u32;
    let width = width_override.unwrap_or(natural_width).clamp(1, 8192);
    let height = height_override.unwrap_or(natural_height).clamp(1, 8192);
    let width = if width_override.is_some() {
        width
    } else {
        width.max(bbox_width)
    };
    let height = if height_override.is_some() {
        height
    } else {
        height.max(bbox_height)
    };

    let mut scene = Scene::new();
    let scene_transform = Affine::translate((-min_x, -min_y));
    // The bundled book: Inter (Figma's default) first, then frame's families.
    frame::render::paint_scene_into(
        &mut scene,
        &doc,
        root,
        scene_transform,
        Some(frame::render::FontBook::bundled()),
    );

    // Offscreen renders like `Renderer::render_to_texture` (MSAA 8×), but
    // re-renders in bands instead of coming back blank when the scene
    // overflows vello's buffers.
    let pixels = frame::render::offscreen::Offscreen::new()?
        .render(&scene, width, height, base_color)?
        .rgba;

    let image = RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| "failed to build image buffer from render output".to_string())?;
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed creating output dir {}: {e}", parent.display()))?;
    }
    image
        .save(&out_path)
        .map_err(|e| format!("failed writing PNG {out_path}: {e}"))?;

    println!(
        "{{\"ok\":true,\"input\":\"{}\",\"out\":\"{}\",\"width\":{},\"height\":{}}}",
        input, out_path, width, height
    );
    Ok(())
}

#[cfg(feature = "anyrender")]
fn extract_export_bbox(
    bytes: &[u8],
    root_figma_id: Option<&str>,
) -> Result<Option<(f64, f64, f64, f64)>, String> {
    let json: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let nodes = match json.get("nodes").and_then(|n| n.as_array()) {
        Some(nodes) if !nodes.is_empty() => nodes,
        _ => return Ok(None),
    };

    let target = if let Some(root_id) = root_figma_id {
        nodes
            .iter()
            .find(|node| node.get("id").and_then(|v| v.as_str()) == Some(root_id))
    } else {
        nodes.first()
    }
    .unwrap_or(&nodes[0]);

    let bbox = target
        .get("absoluteBoundingBox")
        .ok_or_else(|| "export node missing absoluteBoundingBox".to_string())?;
    let x = bbox
        .get("x")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid absoluteBoundingBox.x".to_string())?;
    let y = bbox
        .get("y")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid absoluteBoundingBox.y".to_string())?;
    let w = bbox
        .get("width")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid absoluteBoundingBox.width".to_string())?;
    let h = bbox
        .get("height")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid absoluteBoundingBox.height".to_string())?;
    if w <= 0.0 || h <= 0.0 {
        return Ok(None);
    }
    Ok(Some((x, y, w, h)))
}

#[cfg(feature = "anyrender")]
fn extract_export_base_color(
    bytes: &[u8],
    root_figma_id: Option<&str>,
) -> Result<Option<peniko::Color>, String> {
    let json: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let nodes = match json.get("nodes").and_then(|n| n.as_array()) {
        Some(nodes) if !nodes.is_empty() => nodes,
        _ => return Ok(None),
    };

    let target = if let Some(root_id) = root_figma_id {
        nodes
            .iter()
            .find(|node| node.get("id").and_then(|v| v.as_str()) == Some(root_id))
    } else {
        nodes.first()
    }
    .unwrap_or(&nodes[0]);

    let fills = match target.get("fills").and_then(|v| v.as_array()) {
        Some(fills) => fills,
        None => return Ok(None),
    };
    let solid = match fills
        .iter()
        .find(|fill| fill.get("type").and_then(|v| v.as_str()) == Some("SOLID"))
    {
        Some(fill) => fill,
        None => return Ok(None),
    };
    let color = match solid.get("color").and_then(|v| v.as_object()) {
        Some(color) => color,
        None => return Ok(None),
    };
    let r = color.get("r").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let g = color.get("g").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let b = color.get("b").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let a = solid.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0);

    let to_u8 = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    Ok(Some(peniko::Color::from_rgba8(
        to_u8(r),
        to_u8(g),
        to_u8(b),
        to_u8(a),
    )))
}
