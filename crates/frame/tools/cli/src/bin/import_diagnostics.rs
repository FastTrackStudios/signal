fn main() {
    if let Err(err) = run() {
        eprintln!("import_diagnostics error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or_else(|| "usage: import_diagnostics <figma_export_or_api_json>".to_string())?;
    let bytes = std::fs::read(&input).map_err(|e| format!("failed reading {input}: {e}"))?;
    let (_doc, diagnostics) = frame::figma::rest::import_figma_bytes_with_diagnostics(&bytes)
        .map_err(|e| format!("import failed for {input}: {e}"))?;
    let top = diagnostics
        .top_unsupported_keys(20)
        .into_iter()
        .map(|(key, count)| format!("{{\"key\":\"{}\",\"count\":{}}}", key, count))
        .collect::<Vec<_>>()
        .join(",");

    println!(
        "{{\"input\":\"{}\",\"sourceSchema\":{},\"normalizedAliases\":{},\"unsupportedNodeKeyCount\":{},\"topUnsupportedKeys\":[{}]}}",
        input,
        diagnostics
            .source_schema
            .as_ref()
            .map(|s| format!("\"{}\"", s))
            .unwrap_or_else(|| "null".to_string()),
        diagnostics.normalized_aliases,
        diagnostics.unsupported_node_keys.len(),
        top
    );
    Ok(())
}
