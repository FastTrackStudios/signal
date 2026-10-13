//! Regression tests for VST3 plugin block headers.
//!
//! VST2 headers carry a bare filename token (`reaeq.vst.dylib`); VST3 headers
//! carry a *quoted* one, and VST3 filenames routinely contain spaces
//! (`"Kontakt 8.vst3"`). The header parser used to take the filename with
//! `split_whitespace().next()`, so a VST3 header decayed to
//!
//! ```text
//! <VST "VST3i: Kontakt 8 (Native Instruments) (64 out)" "Kontakt 0 "Kontakt 8.vst3" 0<00> ""
//! ```
//!
//! on the way back out, and REAPER then failed to parse the whole plugin
//! block — including its multi-kilobyte base64 state.

use dawfile_reaper::primitives::parse_rpp;
use dawfile_reaper::types::ReaperProject;
use dawfile_reaper::types::fx_chain::PluginType;
use dawfile_reaper::types::serialize::RppSerialize;

const KONTAKT_HEADER: &str = concat!(
    r#"<VST "VST3i: Kontakt 8 (Native Instruments) (64 out)" "Kontakt 8.vst3" 0 "" "#,
    "952745140{5653544E694B386B6F6E74616B742038} \"\"",
);

const REAEQ_HEADER: &str = "<VST \"VST: ReaEQ (Cockos)\" reaeq.vst.dylib 0 \"\" 1919247985<5653547265657172656165710000000> \"\"";

fn project_with_header(header: &str) -> String {
    format!(
        "<REAPER_PROJECT 0.1 \"7.0\" 0\n  \
         <TRACK\n    NAME \"Keys\"\n    \
         <FXCHAIN\n      SHOW 0\n      LASTSEL 0\n      DOCKED 0\n      BYPASS 0 0 0\n      \
         {header}\n        ZGVhZGJlZWY=\n      >\n      \
         FLOATPOS 0 0 0 0\n      FXID {{AAAA}}\n      WAK 0 0\n    >\n  >\n>\n"
    )
}

fn roundtrip(src: &str) -> String {
    let (_, rpp) = parse_rpp(src).expect("project parses");
    let project = ReaperProject::from_rpp_project(&rpp).expect("project decodes");
    project.to_rpp_string()
}

/// The whole header — quoted filename, flags, custom-name slot, vendor id and
/// plugin state token — must survive parse → serialize byte for byte.
#[test]
fn vst3_quoted_filename_survives_roundtrip() {
    let out = roundtrip(&project_with_header(KONTAKT_HEADER));

    assert!(
        out.contains(KONTAKT_HEADER),
        "VST3 header did not round-trip.\nwanted: {KONTAKT_HEADER}\ngot:\n{out}"
    );
    // The signature of the old corruption: the filename split at its space.
    assert!(
        !out.contains("\"Kontakt 0"),
        "filename was split on its space:\n{out}"
    );
    // The base64 state block must still be inside the plugin block.
    assert!(
        out.contains("ZGVhZGJlZWY="),
        "plugin state was dropped:\n{out}"
    );
}

/// The bare-filename VST2 form must keep working, unquoted.
#[test]
fn vst2_bare_filename_survives_roundtrip() {
    let out = roundtrip(&project_with_header(REAEQ_HEADER));

    assert!(
        out.contains(REAEQ_HEADER),
        "VST2 header did not round-trip.\nwanted: {REAEQ_HEADER}\ngot:\n{out}"
    );
}

/// A second round-trip must not drift — a header re-emitted from the decoded
/// fields has to re-parse into the same fields.
#[test]
fn vst3_header_roundtrip_is_idempotent() {
    let once = roundtrip(&project_with_header(KONTAKT_HEADER));
    let twice = roundtrip(&once);
    assert_eq!(once, twice, "second round-trip changed the output");
}

/// The decoded fields themselves, not just the re-emitted text.
#[test]
fn vst3_header_fields_are_decoded() {
    let src = project_with_header(KONTAKT_HEADER);
    let (_, rpp) = parse_rpp(&src).expect("project parses");
    let project = ReaperProject::from_rpp_project(&rpp).expect("project decodes");

    let chain = project.tracks[0]
        .fx_chain
        .as_ref()
        .expect("track has an FX chain");
    let plugin = match &chain.nodes[0] {
        dawfile_reaper::types::fx_chain::FxChainNode::Plugin(p) => p,
        other => panic!("expected a plugin node, got {other:?}"),
    };

    assert_eq!(plugin.plugin_type, PluginType::Vst3);
    assert_eq!(plugin.file, "Kontakt 8.vst3");
    assert_eq!(
        plugin.name,
        "VST3i: Kontakt 8 (Native Instruments) (64 out)"
    );
    assert_eq!(plugin.custom_name, None);

    let extra = plugin
        .header_extra
        .as_ref()
        .expect("VST3 header keeps its surrounding fields");
    assert_eq!(extra.flags, "0");
    assert_eq!(
        extra.tail,
        "952745140{5653544E694B386B6F6E74616B742038} \"\""
    );
}

/// A renamed VST3 FX still re-emits a well-formed header: the new custom name
/// goes into its own slot, the vendor id and state token stay put.
#[test]
fn vst3_custom_name_is_re_emitted_in_its_own_slot() {
    let src = project_with_header(KONTAKT_HEADER);
    let (_, rpp) = parse_rpp(&src).expect("project parses");
    let mut project = ReaperProject::from_rpp_project(&rpp).expect("project decodes");

    let chain = project.tracks[0]
        .fx_chain
        .as_mut()
        .expect("track has an FX chain");
    match &mut chain.nodes[0] {
        dawfile_reaper::types::fx_chain::FxChainNode::Plugin(p) => {
            p.custom_name = Some("Main Keys".to_string());
        }
        other => panic!("expected a plugin node, got {other:?}"),
    }

    let out = project.to_rpp_string();
    assert!(
        out.contains(
            "\"Kontakt 8.vst3\" 0 \"Main Keys\" 952745140{5653544E694B386B6F6E74616B742038} \"\""
        ),
        "renamed VST3 header is malformed:\n{out}"
    );
}

// ──────────────────────────────────────────────────────────────
// Against the real 1.8 MB project that first showed the corruption.
// ──────────────────────────────────────────────────────────────

const ROCKSTARS_RPP: &str =
    "/run/media/AudioHaven/Project/Crescendum-Rockstars/it knows my name/it knows my name.RPP";

/// Every `<VST` header in a real, plugin-heavy project must come back out
/// unchanged.
#[test]
fn real_project_vst_headers_survive_roundtrip() {
    let Ok(original) = std::fs::read_to_string(ROCKSTARS_RPP) else {
        eprintln!("Skipping: Rockstars RPP not found at {ROCKSTARS_RPP}");
        return;
    };

    let out = roundtrip(&original);

    let headers: Vec<&str> = original
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("<VST "))
        .collect();
    assert!(
        !headers.is_empty(),
        "expected the real project to contain VST headers"
    );

    let mut lost: Vec<&str> = headers
        .iter()
        .copied()
        .filter(|h| !out.contains(*h))
        .collect();
    lost.dedup();
    assert!(
        lost.is_empty(),
        "{} of {} VST headers were corrupted, e.g.:\n{}",
        lost.len(),
        headers.len(),
        lost.iter().take(5).copied().collect::<Vec<_>>().join("\n")
    );
}
