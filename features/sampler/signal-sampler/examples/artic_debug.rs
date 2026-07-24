//! Why did the engine pick that articulation? Prints each articulation's
//! kind, zone count and whether the selector treats it as an aux layer.

use std::path::Path;

use signal_sampler::PlayerPatch;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pack = std::env::args().nth(1).expect("usage: artic_debug <pack>");
    let patch = PlayerPatch::from_pack(Path::new(&pack))?;
    let spec = &patch.spec;
    println!("{} — {} zones", spec.name, spec.zones.len());
    for a in &spec.articulations {
        let zones = spec.zones.iter().filter(|z| z.articulation == a.id).count();
        let l = a.id.to_ascii_lowercase();
        let aux = l.contains("mch") || l.contains("mech") || l.contains("ped") || l.contains("pdl");
        println!(
            "  {:<14} kind={:<10?} zones={:<6} aux={}",
            a.id, a.kind, zones, aux
        );
    }
    println!(
        "selected: {:?}",
        signal_sampler::engine::default_articulation(spec)
    );
    Ok(())
}
