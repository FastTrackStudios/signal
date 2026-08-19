//! `gig_extract` — read a Gig Performer `.gig` file: list the rig, or dump a
//! hosted plugin's state (and, for Omnisphere, its Multi XML + patch names).
//!
//! ```text
//! gig_extract list  <file.gig>                 rig map: rackspace / block / plugin
//! gig_extract omni  <file.gig>                 every Omnisphere part, by name
//! gig_extract dump  <file.gig> <out_dir>       write each plugin chunk + omni XML
//! ```
//!
//! Pairs with `omni_state`: the XML this writes is the same `SynthMaster`
//! Multi dialect, so `omni_state wrap` turns it straight back into loadable
//! plugin state.

use signal_synth::gig::{read_gig, GigProcessor};

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

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

fn load(path: &str) -> Vec<GigProcessor> {
    let xml = std::fs::read_to_string(path).expect("read gig file");
    read_gig(&xml)
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
                }
                println!("{stem}  ({} bytes)", p.state.len());
            }
        }
        _ => {
            eprintln!("usage: gig_extract list|omni <file.gig>");
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
