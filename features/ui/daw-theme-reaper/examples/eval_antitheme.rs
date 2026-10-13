//! Evaluate the Anti-Theme's WALTER program and dump resolved mcp.* attrs.
fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| {
        "/home/cody/Development/FastTrackStudio/reaper-theme/extracted/antitheme".into()
    });
    let theme = daw_theme_reaper::ReaperTheme::load_dir(&dir).unwrap();
    let rt =
        std::fs::read_to_string(std::path::Path::new(&theme.images.dir()).join("rtconfig.txt"))
            .unwrap();

    let mut env = daw_theme_reaper::walter::Env::new();
    // Panel size + REAPER track scalars (a plain stereo track, not selected).
    for (k, v) in [
        ("w", 110.0),
        ("h", 600.0),
        ("trackpanmode", 3.0),
        ("tracknch", 2.0),
        ("recarm", 0.0),
        ("track_selected", 0.0),
        ("folderstate", 0.0),
        ("folderdepth", 0.0),
        ("maxfolderdepth", 0.0),
        ("mcp_maxfolderdepth", 0.0),
        ("trackcolor_valid", 1.0),
        ("trackcolor_r", 200.0),
        ("trackcolor_g", 80.0),
        ("trackcolor_b", 40.0),
        ("trackidx", 1.0),
        ("ntracks", 9.0),
        ("mixer_visible", 1.0),
        ("send_cnt", 0.0),
        ("fx_cnt", 0.0),
        ("fx_parm_cnt", 0.0),
        ("recfx_cnt", 0.0),
        ("mcp_wantextmix", 0.0),
        ("tcp_sends_enabled", 1.0),
        ("tcp_fxlist_enabled", 1.0),
        ("tcp_fxparms", 0.0),
        ("tcp_fxembed", 0.0),
        ("mcp_fxembed", 0.0),
        ("trackfixedlanes", 0.0),
        ("trackpinned", 0.0),
        ("tcp_hidden_overridden", 0.0),
        ("reaper_version", 7.0),
        ("os_type", 2.0),
        ("Scale", 1.0),
    ] {
        env.set(k, v);
    }
    // define_parameter defaults become variables.
    for p in &theme.rtconfig.params {
        env.set(&p.name, p.default);
    }

    let out = daw_theme_reaper::walter::evaluate(&rt, std::env::args().nth(2).as_deref(), &env);
    println!("layouts: {:?}", out.layouts);
    for probe in [
        "mcpWidth",
        "mcpForm",
        "mcpNchanGrowPx",
        "mcpFormW",
        "previous",
        "sidebarWidth",
        "hide_mcp.mute",
        "biggestY",
        "thisWs",
        "thisHs",
        "thisBS",
        "sectionButtonsBS",
        "mcpIoH",
        "mcp.custom.sectionButtons",
        "mcp.custom.sectionPan",
        "mcp.custom.mcpDarkBox",
        "mcpVolKnobH",
        "sectionBottomH",
        "Scale",
        "sidebarWidth",
        "mcpNameH",
        "sectionTopH",
    ] {
        println!("{probe} = {:?}", out.get(probe));
    }
    let prefix = std::env::args().nth(3).unwrap_or_else(|| "mcp.".into());
    let mut keys: Vec<&String> = out
        .attrs
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .collect();
    keys.sort();
    println!("{prefix}* attrs: {}", keys.len());
    for k in keys.iter() {
        let v = &out.attrs[*k];
        let head: Vec<String> = v.iter().take(8).map(|x| format!("{x:.0}")).collect();
        println!("  {k} = [{}]", head.join(" "));
    }
}
