//! REAPER integration test: capture full live parameter lists for NDSP archetypes.
//!
//! Run with:
//!   cargo xtask reaper-test reaper_ndsp_param_inventory

use std::fs;
use std::path::PathBuf;

use reaper_test::reaper_test;

const NDSP_VST3_PLUGINS: &[&str] = &[
    "VST3: Archetype Cory Wong X (Neural DSP)",
    "VST3: Archetype John Mayer X (Neural DSP)",
    "VST3: Archetype Misha Mansoor X (Neural DSP)",
    "VST3: Archetype Nolly X (Neural DSP)",
    "VST3: Archetype Petrucci X (Neural DSP)",
    "VST3: Archetype Rabea X (Neural DSP)",
    "VST3: Archetype Tim Henson X (Neural DSP)",
];

fn slugify(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[reaper_test]
async fn capture_ndsp_full_parameter_lists(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let report_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/reaper-outputs");
    fs::create_dir_all(&report_dir)?;

    let track = project
        .tracks()
        .add("NDSP Param Inventory", None)
        .await
        .map_err(|e| eyre::eyre!("create inventory track: {e}"))?;

    let mut combined_report = String::new();
    combined_report.push_str("# Neural DSP Parameter Inventory (Live REAPER)\n\n");

    for plugin_name in NDSP_VST3_PLUGINS {
        let fx = track
            .fx_chain()
            .add(plugin_name)
            .await
            .map_err(|e| eyre::eyre!("add plugin '{plugin_name}': {e}"))?;

        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        let info = fx
            .info()
            .await
            .map_err(|e| eyre::eyre!("read FX info for '{plugin_name}': {e}"))?;
        let params = fx
            .parameters()
            .await
            .map_err(|e| eyre::eyre!("read FX parameters for '{plugin_name}': {e}"))?;

        println!(
            "[ndsp-param-inventory] {} -> {} params",
            plugin_name,
            params.len()
        );

        let mut plugin_report = String::new();
        plugin_report.push_str(&format!("# {}\n", plugin_name));
        plugin_report.push_str(&format!(
            "- plugin_name: {}\n- parameter_count: {}\n\n",
            info.plugin_name,
            params.len()
        ));
        plugin_report.push_str("## Parameters\n");
        for p in &params {
            plugin_report.push_str(&format!(
                "- [{}] {} | value={:.6} | formatted={}\n",
                p.index, p.name, p.value, p.formatted
            ));
        }
        plugin_report.push('\n');

        let file_name = format!("{}.md", slugify(plugin_name));
        let plugin_path = report_dir.join(file_name);
        fs::write(&plugin_path, &plugin_report)?;

        combined_report.push_str(&format!("## {}\n", plugin_name));
        combined_report.push_str(&format!("- parameter_count: {}\n", params.len()));
        combined_report.push_str(&format!(
            "- report: {}\n\n",
            plugin_path.to_string_lossy()
        ));
    }

    let combined_path = report_dir.join("ndsp-full-parameter-list.md");
    fs::write(&combined_path, combined_report)?;
    println!(
        "[ndsp-param-inventory] wrote combined report: {}",
        combined_path.display()
    );

    Ok(())
}
