//! `dump_patch` — import a `.prt_omn` and print each layer's filter, filter
//! envelope, and the mod routes that drive the cutoff. Used to reason about why
//! a held note chokes (filter cutoff/envelope calibration).
//!
//! ```text
//! cargo run -p signal-synth --release --example dump_patch -- "<patch.prt_omn>"
//! ```

use signal_sampler::rig_node::{Container, RigNode};
use signal_synth::omni_import::{SoundsourceIndex, load_patch_file};

fn walk<'a>(c: &'a Container, out: &mut Vec<&'a Container>) {
    out.push(c);
    for ch in &c.children {
        if let RigNode::Container { container } = ch {
            walk(container, out);
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_patch <patch.prt_omn>");
    let index = SoundsourceIndex::scan_default();
    dump_raw(std::path::Path::new(&path));
    let tree = load_patch_file(std::path::Path::new(&path), &index).expect("import");
    println!("patch: {}", tree.name);

    let mut containers = Vec::new();
    walk(&tree, &mut containers);

    for c in &containers {
        if !c.name.starts_with("Layer") {
            continue;
        }
        println!("\n=== {} ===", c.name);
        // filter blocks
        if let Some(filters) = c.find("Filters") {
            for b in filters.blocks() {
                let cutoff = b.param_f32("cutoff");
                let res = b.param_f32("resonance");
                if cutoff.is_some() || res.is_some() {
                    let hz = cutoff.map(|n| 20.0 * 10f32.powf(n * 3.0));
                    println!(
                        "  filter '{}': cutoff_norm={:?} (~{:.0} Hz)  res={:?}  mode={:?} poles={:?}",
                        b.display_name(),
                        cutoff,
                        hz.unwrap_or(0.0),
                        res,
                        b.param_str("mode"),
                        b.param_str("poles"),
                    );
                }
            }
        }
        // filter env
        for m in &c.modulators {
            if m.display_name() == "Filter Env" {
                println!(
                    "  Filter Env ADSR: a={:?} d={:?} s={:?} r={:?}",
                    m.param_f32("attack"),
                    m.param_f32("decay"),
                    m.param_f32("sustain"),
                    m.param_f32("release"),
                );
            }
        }
        // mod routes on this layer
        for rt in &c.mod_routes {
            println!("  route: {} -> {}  depth={:.3}", rt.source, rt.target, rt.depth);
        }
    }
}

/// Raw parse view — the layers as the importer sees them, plus the part LFOs
/// and mod matrix (what a module preset has to carry).
fn dump_raw(path: &std::path::Path) {
    let xml = std::fs::read_to_string(path).expect("read");
    let p = signal_synth::omni_import::parse_patch(&xml).expect("parse");
    println!("\n--- raw parse: {} ({} layers) ---", p.name, p.layers.len());
    for (i, l) in p.layers.iter().enumerate() {
        println!(
            "  L{i}: ss={:?} lib={:?} lvl={:.3} filt={}({:.3}/{:.3}) act={} uni={}x{:.2} amp={:?} fenv={:?} fdepth={:.3} fx={:?}",
            l.soundsource, l.ss_library, l.level, l.filter_name, l.filter_freq, l.filter_res,
            l.filter_active, l.unison_count, l.unison_detune, l.amp_env, l.filter_env,
            l.filter_env_depth, l.fx,
        );
    }
    println!("  lfos: {:?}", p.lfos);
    for r in &p.mod_routes {
        println!("  route {} -> {} @ {:.3}", r.source, r.target, r.depth);
    }
}
