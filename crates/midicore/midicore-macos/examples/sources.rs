//! Every CoreMIDI source with its unique id and offline flag — what the
//! backend's rescan sees.
//! `cargo run -p midicore-macos --example sources`

#[cfg(target_os = "macos")]
fn main() {
    for s in coremidi::Sources {
        let name = s.display_name().or_else(|| s.name()).unwrap_or_default();
        let offline: Result<bool, _> = s.get_property(&coremidi::Properties::offline());
        println!(
            "{:>12}  offline={:?}  {name}",
            s.unique_id().unwrap_or(0),
            offline
        );
    }
}

/// CoreMIDI is macOS's; elsewhere there is nothing to list.
#[cfg(not(target_os = "macos"))]
fn main() {}
