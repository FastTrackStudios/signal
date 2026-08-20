//! `gig_extract` — read a Gig Performer `.gig` file: list the rig, or dump a
//! hosted plugin's state (and, for Omnisphere, its Multi XML + patch names).
//!
//! ```text
//! gig_extract list    <file.gig>               rig map: rackspace / block / plugin
//! gig_extract omni    <file.gig>               every Omnisphere part, by name
//! gig_extract presets <file.gig>               patches + which sources each enables
//! gig_extract songs   <file.gig>               song library + setlists
//! gig_extract dump    <file.gig> <out_dir>     write each plugin chunk + omni XML
//! ```
//!
//! Pairs with `omni_state`: the XML this writes is the same `SynthMaster`
//! Multi dialect, so `omni_state wrap` turns it straight back into loadable
//! plugin state.

use signal_synth::gig::{read_gig, GigProcessor};

/// The eight source-select toggles on the global rackspace: the four NI pianos
/// and the four Keyscape layers. A preset turning one on is the *only* record
/// of which instrument that patch actually plays.
const SOURCES: [&str; 8] = [
    "The Grandeur",
    "The Gentleman",
    "The Maverick",
    "The Giant",
    "Felt",
    "Wing",
    "Vintage",
    "Rhodes",
];

/// Continuous macros worth carrying across — the ones a patch actually dials.
const MACROS: [&str; 5] = ["Color", "Comp", "Cutoff", "Envelope", "Reverb"];

/// Every `<ENTRYDESCR name= library=>` in a Multi, in part order. Part 0 is
/// the Multi's own descriptor; parts 1..8 are the slots.
fn part_names(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<ENTRYDESCR ") {
        rest = &rest[at..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        let grab = |k: &str| -> String {
            let needle = format!("{k}=\"");
            tag.find(&needle)
                .map(|i| i + needle.len())
                .and_then(|s| tag[s..].find('"').map(|e| tag[s..s + e].to_string()))
                .unwrap_or_default()
        };
        out.push((grab("name"), grab("library")));
        rest = &rest[end..];
    }
    out
}

/// Split a `SynthMaster` Multi into its parts, each wrapped back up as a
/// standalone `AmberPart` document — the `.prt_omn` form.
///
/// A part is one `<SynthSubEngine>`; its `<SynthEngine>` subtree is exactly
/// what a `.prt_omn` holds, so re-wrapping is all it takes to turn a part of a
/// Multi back into a patch file the ordinary reader accepts.
fn split_parts(multi_xml: &str) -> Vec<Option<(String, String)>> {
    let mut out = Vec::new();
    let mut rest = multi_xml;
    while let Some(at) = rest.find("<SynthSubEngine") {
        rest = &rest[at..];
        let end = match rest.find("</SynthSubEngine>") {
            Some(e) => e + "</SynthSubEngine>".len(),
            None => break,
        };
        let part = &rest[..end];
        rest = &rest[end..];

        let engine = part.find("<SynthEngine").and_then(|s| {
            part.rfind("</SynthEngine>")
                .map(|e| &part[s..e + "</SynthEngine>".len()])
        });
        let name = part
            .find("<ENTRYDESCR ")
            .and_then(|s| part[s..].find('>').map(|e| &part[s..s + e]))
            .and_then(|tag| {
                tag.find("name=\"")
                    .map(|i| i + 6)
                    .and_then(|i| tag[i..].find('"').map(|e| tag[i..i + e].trim().to_string()))
            })
            .unwrap_or_default();
        // An empty slot is not a patch.
        out.push(match (engine, name.as_str()) {
            (Some(e), n) if !n.is_empty() && n != "Empty" => {
                Some((name.clone(), format!("<AmberPart >\n{e}\n</AmberPart>\n")))
            }
            _ => None,
        });
    }
    out
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

fn load(path: &str) -> Vec<GigProcessor> {
    let xml = std::fs::read_to_string(path).expect("read gig file");
    read_gig(&xml)
}

fn presets(path: &str) {
    let xml = std::fs::read_to_string(path).expect("read gig file");
    let presets = signal_synth::gig::read_presets(&xml);
    println!(
        "{:<30} {:<20} {:<38} MACROS",
        "PATCH", "RACKSPACE", "SOURCES"
    );
    for p in &presets {
        let on = p.enabled(&SOURCES).join(", ");
        let macros: Vec<String> = MACROS
            .iter()
            .filter_map(|m| p.get(m).map(|v| format!("{m}={v:.2}")))
            .collect();
        println!(
            "{:<30} {:<20} {:<38} {}",
            p.name,
            p.rackspace,
            if on.is_empty() { "(none)".into() } else { on },
            macros.join(" ")
        );
    }
    // The point of the whole exercise: Gig Performer keeps every source
    // resident; a patch needs two.
    let widest = presets
        .iter()
        .map(|p| p.enabled(&SOURCES).len())
        .max()
        .unwrap_or(0);
    let used: std::collections::BTreeSet<&str> =
        presets.iter().flat_map(|p| p.enabled(&SOURCES)).collect();
    println!("\n{} patches; widest uses {widest} sources", presets.len());
    println!(
        "sources ever used: {}",
        used.iter().copied().collect::<Vec<_>>().join(", ")
    );
    let never: Vec<&str> = SOURCES
        .iter()
        .copied()
        .filter(|s| !used.contains(s))
        .collect();
    if !never.is_empty() {
        println!("NEVER used by any patch: {}", never.join(", "));
    }
}

fn songs(path: &str) {
    let xml = std::fs::read_to_string(path).expect("read gig file");
    let all = signal_synth::gig::read_songs(&xml);
    let setlists = signal_synth::gig::read_setlists(&xml);

    let mut seen = std::collections::BTreeMap::new();
    for s in &all {
        if !s.name.trim().is_empty() {
            seen.entry(s.name.clone()).or_insert(s);
        }
    }
    println!(
        "{:<38} {:<6} {:<6} {:<6} {:<5} PARTS",
        "TITLE", "KEY", "STORED", "BPM", "SIG"
    );
    let mut disagree = 0;
    for s in seen.values() {
        let named = s.key_from_name().unwrap_or("-");
        let stored = s.stored_key();
        if named != "-" && named != stored {
            disagree += 1;
        }
        println!(
            "{:<38} {:<6} {:<6} {:<6.1} {:<5} {}",
            s.title(),
            named,
            stored,
            s.bpm,
            format!("{}/{}", s.sig_num, s.sig_den),
            s.parts.len()
        );
    }
    println!(
        "\n{} unique songs ({} setlist entries), {} setlists",
        seen.len(),
        all.len(),
        setlists.len()
    );
    if disagree > 0 {
        println!(
            "{disagree} song(s) where rootNote disagrees with the title key — trust the title"
        );
    }
    println!("\nSETLISTS");
    for sl in &setlists {
        println!("  {:<28} {}", sl.name, sl.songs.len());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") => {
            let mut current = String::new();
            for p in load(&args[1]) {
                if p.rackspace != current {
                    current = p.rackspace.clone();
                    println!("\n=== {current}");
                }
                println!(
                    "  {:<34} {:<26} {:>9} bytes",
                    p.node_name,
                    p.plugin,
                    p.state.len()
                );
            }
        }
        Some("omni") => {
            for p in load(&args[1]) {
                let Some(xml) = p.omni_multi_xml() else {
                    continue;
                };
                println!("\n=== [{}] {}", p.rackspace, p.node_name);
                // The Multi parsed properly, so each part's layers (and the
                // soundsource each one loads) come out too — which is what you
                // actually need to rebuild the patch, not just its name.
                let parsed = signal_synth::omni_import::parse_multi(&xml).ok();
                for (i, (name, library)) in part_names(&xml).into_iter().enumerate() {
                    if name.is_empty() || name == "Empty" {
                        continue;
                    }
                    let slot = if i == 0 {
                        "multi".to_string()
                    } else {
                        format!("part {i}")
                    };
                    println!("  {slot:<7} {name}   [{library}]");
                    // Part n is index n-1 in the parts list; the multi's own
                    // descriptor is not a part.
                    let Some(part) = parsed
                        .as_ref()
                        .filter(|_| i > 0)
                        .and_then(|m| m.parts.get(i - 1))
                    else {
                        continue;
                    };
                    let (patch, level, muted) = part;
                    for (n, layer) in patch.layers.iter().enumerate() {
                        if layer.soundsource.is_empty() {
                            continue;
                        }
                        println!(
                            "            layer {}: {}   [{}]{}",
                            (b'A' + n as u8) as char,
                            layer.soundsource,
                            layer.ss_library,
                            if layer.filter_active {
                                format!("  filter {}", layer.filter_name)
                            } else {
                                String::new()
                            }
                        );
                    }
                    println!(
                        "            level {:.2}{}",
                        level,
                        if *muted { "  (MUTED)" } else { "" }
                    );
                }
            }
        }
        Some("dump") => {
            let dir = std::path::Path::new(&args[2]);
            std::fs::create_dir_all(dir).expect("create out dir");
            for p in load(&args[1]) {
                let stem = format!("{}__{}", slug(&p.rackspace), slug(&p.node_name));
                std::fs::write(dir.join(format!("{stem}.chunk")), &p.state).expect("write chunk");
                if let Some(xml) = p.omni_multi_xml() {
                    std::fs::write(dir.join(format!("{stem}.multi.xml")), &xml)
                        .expect("write multi xml");
                    // Each part back out as a standalone `.prt_omn`. Some of
                    // these exist NOWHERE else: the rig's "Worship PHAT Bass"
                    // was never saved to the Spectrasonics user library, so
                    // the gig's plugin state is its only copy. Writing them as
                    // files makes them ours instead of hostage to one .gig.
                    //
                    // Named by the PATCH, not by where it sat: in a patch
                    // library the name is the identity, and that is the name
                    // a profile and the rig's own scanner look it up by. A
                    // patch loaded into two rackspaces is one patch, so the
                    // first write wins rather than the last.
                    for part in split_parts(&xml).into_iter().flatten() {
                        let (name, body) = part;
                        let out = dir.join(format!("{name}.prt_omn"));
                        if !out.exists() {
                            std::fs::write(&out, body).expect("write patch");
                        }
                    }
                }
                println!("{stem}  ({} bytes)", p.state.len());
            }
        }
        Some("presets") => presets(&args[1]),
        Some("songs") => songs(&args[1]),
        _ => {
            eprintln!("usage: gig_extract list|omni|presets|songs <file.gig>");
            eprintln!("       gig_extract dump <file.gig> <out_dir>");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    /// The Multi XML recovered from a gig must be consumable by the ordinary
    /// `.mlt_omn` reader — that is the whole point of unwrapping it. Machine-
    /// local: needs the gig file, so `#[ignore]`d like the other host tests.
    #[test]
    #[ignore = "needs a local .gig file"]
    fn recovered_multi_parses() {
        let path = std::env::var("GIG_FILE").expect("set GIG_FILE");
        let xml = std::fs::read_to_string(path).unwrap();
        let procs = signal_synth::gig::read_gig(&xml);
        let mut checked = 0;
        for p in &procs {
            let Some(multi_xml) = p.omni_multi_xml() else {
                continue;
            };
            let multi = signal_synth::omni_import::parse_multi(&multi_xml)
                .unwrap_or_else(|e| panic!("{} / {}: {e}", p.rackspace, p.node_name));
            assert_eq!(multi.parts.len(), 8, "{}: 8 parts", p.node_name);
            checked += 1;
        }
        assert!(
            checked >= 5,
            "expected the gig's Omnisphere instances, got {checked}"
        );
    }
}
