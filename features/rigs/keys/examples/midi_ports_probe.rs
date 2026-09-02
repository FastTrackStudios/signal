//! Does the MIDI backend see the hardware? Prints the input ports midicore
//! enumerates, which is what the keys rig attaches to.
fn main() {
    let ports = midicore::pipewire::input_ports();
    println!("input_ports() -> {} port(s)", ports.len());
    for p in &ports {
        println!("  - {p}");
    }
    if ports.is_empty() {
        println!("NO PORTS — the rig has nothing to attach to.");
        std::process::exit(1);
    }
}
