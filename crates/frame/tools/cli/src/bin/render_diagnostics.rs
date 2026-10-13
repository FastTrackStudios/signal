fn main() {
    if let Err(err) = run() {
        eprintln!("render_diagnostics error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or_else(|| "usage: render_diagnostics <figma_export_or_api_json>".to_string())?;
    let bytes = std::fs::read(&input).map_err(|e| format!("failed reading {input}: {e}"))?;
    let doc = figma_import::import_bytes(&bytes)
        .map_err(|e| format!("import failed for {input}: {e}"))?;
    let root = doc.pages().first().copied().unwrap_or(doc.root);
    let diag = render::collect_render_diagnostics(&doc, root);

    println!(
        "{{\"input\":\"{}\",\"root\":\"{}\",\"summary\":\"{}\",\"diagnostics\":{{\"totalNodes\":{},\"nonNormalBlendNodes\":{},\"gradientFillNodes\":{},\"imageFillNodes\":{},\"dropShadowNodes\":{},\"innerShadowNodes\":{},\"layerBlurNodes\":{},\"layerBlurApproxNodes\":{},\"backgroundBlurNodes\":{},\"backgroundBlurApproxNodes\":{},\"alphaMaskNodes\":{},\"luminanceMaskNodes\":{}}}}}",
        input,
        doc.node(root)
            .and_then(|n| n.source.as_ref())
            .map(|s| s.id.clone())
            .unwrap_or_default(),
        diag.format_summary().replace('\"', "\\\""),
        diag.total_nodes,
        diag.non_normal_blend_nodes,
        diag.gradient_fill_nodes,
        diag.image_fill_nodes,
        diag.drop_shadow_nodes,
        diag.inner_shadow_nodes,
        diag.layer_blur_nodes,
        diag.layer_blur_approx_nodes,
        diag.background_blur_nodes,
        diag.background_blur_approx_nodes,
        diag.alpha_mask_nodes,
        diag.luminance_mask_nodes
    );

    Ok(())
}
