//! Offline proof of the Electronic Kit play path (#77 M3): point the rig at
//! a built space, generate a kit, render each pad's hit, and report which
//! pads actually made sound — then morph the kit and confirm the samples
//! changed.
//!
//! ```bash
//! cargo run --release -p signal-ekit --example pad_probe -- <space-name>
//! ```

use signal_ekit::proto::ekit::EkitRig;
use signal_ekit::EkitBackend;

fn main() {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let space = std::env::args().nth(1).unwrap_or_else(|| "luke-pieces".into());

    let b = EkitBackend::new_offline(48_000);
    EkitRig::set_space(&b, space.clone());
    let st = EkitRig::status(&b);
    if st.space.is_empty() {
        eprintln!("no space {space:?} — build one first (space_cli build … --pieces)");
        std::process::exit(1);
    }
    let pads = EkitRig::pads(&b);
    println!("space {:?}: {} pads", st.space, pads.len());
    for p in &pads {
        println!("  pad {:>2} [{:<10}] {}", p.index, p.category, p.path);
    }

    let loaded = pads.iter().filter(|p| !p.path.is_empty()).count();
    println!("\n{loaded}/{} pads filled", pads.len());

    // Render each pad and measure.
    let mut audible = 0usize;
    for p in &pads {
        if p.path.is_empty() {
            continue;
        }
        let peak = b.render_hit(p.index, 110);

        if peak > 1e-4 {
            audible += 1;
        }
        println!("  pad {:>2} peak {peak:.4}  {}", p.index, p.path);
    }
    println!("\n{audible}/{loaded} filled pads audible");

    // Morph and confirm the kit moved.
    let before: Vec<String> = pads.iter().map(|p| p.path.clone()).collect();
    EkitRig::morph_kit(&b, 1);
    let after: Vec<String> = EkitRig::pads(&b).iter().map(|p| p.path.clone()).collect();
    let changed = before.iter().zip(&after).filter(|(a, b)| a != b).count();
    println!("morph_kit(+1): {changed}/{} pads moved to a neighbour", before.len());

    if audible == 0 {
        eprintln!("FAIL — no pad produced audio");
        std::process::exit(1);
    }
    println!("PASS");
}
