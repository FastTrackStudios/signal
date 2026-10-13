//! Print the Nord Stage 4 placeholder routing tree.
//!
//! ```text
//! cargo run -p signal-sampler --example nord_routing
//! ```

fn main() {
    let preset = signal_sampler::nord::nord_stage_preset();
    println!("{}", preset.dump());
}
