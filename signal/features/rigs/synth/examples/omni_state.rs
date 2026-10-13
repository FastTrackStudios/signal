//! `omni_state` — build / unwrap Omnisphere VST3 state chunks from the CLI
//! (the Rust home of what the throwaway python probes did during the format
//! reverse-engineering). Pairs with `load_plugin --load-state`.
//!
//! ```text
//! omni_state wrap   <multi.xml|.mlt_omn> <out_state.bin>
//! omni_state unwrap <state.bin> <out.xml>
//! omni_state patch  <patch.prt_omn> <template.(xml|bin)> <out_state.bin> [ATTR=VAL ...]
//! ```
//!
//! `patch` splices the patch into Part 1 of the template Multi (a state dump
//! or an XML), applying `ATTR=VAL` rewrites first — the calibration sweep
//! hook (VAL is the raw attribute string, IEEE-754 hex for floats).

use signal_synth::omni_import::state;

fn read_xml(path: &str) -> String {
    let bytes = std::fs::read(path).expect("read input");
    if bytes.starts_with(b"DAW3") {
        state::parse_state(&bytes).expect("parse state chunk")
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("wrap") => {
            let xml = read_xml(&args[1]);
            let chunk = state::build_state(&xml);
            std::fs::write(&args[2], &chunk).expect("write state");
            println!("{}: {} bytes", args[2], chunk.len());
        }
        Some("unwrap") => {
            let xml = read_xml(&args[1]);
            std::fs::write(&args[2], &xml).expect("write xml");
            println!("{}: {} chars", args[2], xml.len());
        }
        Some("patch") => {
            let mut patch = std::fs::read_to_string(&args[1]).expect("read patch");
            for rewrite in &args[4..] {
                let (attr, val) = rewrite.split_once('=').expect("ATTR=VAL");
                let (next, n) = state::rewrite_attr(&patch, attr, val);
                println!("  {attr} -> {val} ({n} sites)");
                patch = next;
            }
            let template = read_xml(&args[2]);
            let multi = state::patch_into_multi(&patch, &template).expect("splice");
            let chunk = state::build_state(&multi);
            std::fs::write(&args[3], &chunk).expect("write state");
            println!("{}: {} bytes", args[3], chunk.len());
        }
        _ => {
            eprintln!("usage: omni_state wrap|unwrap|patch … (see module docs)");
            std::process::exit(2);
        }
    }
}
