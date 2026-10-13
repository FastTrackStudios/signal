//! Refuse a built-in microphone as a live input.
//!
//! A live rig monitors its input through its output. On a laptop with nothing
//! plugged in, "the default devices" are the machine's own microphone and its
//! own speakers — so the rig plays the speakers into the mic, through the amp
//! and FX, and back out of the speakers, and feeds back without limit the
//! instant it opens. That is not a setting anyone wants by accident, so every
//! path that opens an input asks [`check_input`] first:
//!
//! - the cpal path ([`open_input`](crate::open_input)),
//! - the CoreAudio duplex backend (by the device's own transport type),
//! - the PipeWire duplex linker (by the capture node it would link).
//!
//! [`AudioIoPrefs::allow_builtin_mic`](crate::AudioIoPrefs::allow_builtin_mic)
//! turns the guard off for the rare case that wants it (headphones on, testing
//! with the laptop mic).
//!
//! # How a built-in mic is recognised
//!
//! On macOS a device found in CoreAudio is judged by its transport type —
//! `kAudioDeviceTransportTypeBuiltIn` — which is exact. Everywhere else, and
//! for a name CoreAudio does not know, [`name_says_builtin_mic`] matches the
//! names built-in inputs go by: "MacBook Pro Microphone", "Built-in
//! Microphone", PipeWire's "Built-in Audio" and its `alsa_input.pci-…` nodes
//! (the motherboard codec — a laptop's internal mic, a desktop's mic jack).
//! Erring toward refusing is deliberate: the override is one setting away,
//! and a false negative is a feedback loop at full volume.

/// Does `name` (a device name or a PipeWire node name) look like a built-in
/// microphone?
#[must_use]
pub fn name_says_builtin_mic(name: &str) -> bool {
    let n = name.to_lowercase();
    let mic = n.contains("microphone") || n.contains(" mic");
    // Apple's own: "MacBook Pro Microphone", "iMac Microphone", "Studio
    // Display Microphone" (a display's mic sits next to its speakers too).
    let apple = ["macbook", "imac", "studio display"]
        .iter()
        .any(|m| n.contains(m))
        && mic;
    let builtin = (n.contains("built-in") || n.contains("builtin") || n.contains("internal"))
        && (mic || n.contains("audio"));
    // Laptop DMICs and the onboard HDA codec as PipeWire names them.
    let onboard =
        n.contains("digital microphone") || n.contains("dmic") || n.starts_with("alsa_input.pci-");
    apple || builtin || onboard
}

/// Is the input called `name` a built-in microphone?
#[must_use]
pub fn is_builtin_microphone(name: &str) -> bool {
    #[cfg(target_os = "macos")]
    if let Some(builtin) = crate::duplex_coreaudio::input_is_builtin(name) {
        return builtin;
    }
    name_says_builtin_mic(name)
}

/// [`check_input`] for a caller that has already classified the device (the
/// CoreAudio backend reads the transport type itself).
///
/// # Errors
///
/// As [`check_input`].
pub fn check_input_known(name: &str, builtin: bool, allow_builtin_mic: bool) -> Result<(), String> {
    if allow_builtin_mic || !builtin {
        return Ok(());
    }
    refuse(name)
}

/// `Ok` if the input called `name` may be opened as a live input.
///
/// # Errors
///
/// When `name` is a built-in microphone and `allow_builtin_mic` is off. The
/// message says which device and how to allow it.
pub fn check_input(name: &str, allow_builtin_mic: bool) -> Result<(), String> {
    if allow_builtin_mic || !is_builtin_microphone(name) {
        return Ok(());
    }
    refuse(name)
}

fn refuse(name: &str) -> Result<(), String> {
    tracing::warn!(
        audio.input = %name,
        audio.guard = "builtin_mic",
        "refused the built-in microphone as a live input"
    );
    Err(format!(
        "refusing to open the built-in microphone \"{name}\" as a live input — \
         monitoring it through the speakers feeds back. Connect an audio \
         interface and select it, or set allow_builtin_mic to override."
    ))
}

#[cfg(test)]
mod tests {
    use super::name_says_builtin_mic as builtin;

    #[test]
    fn apple_built_in_microphones_are_recognised() {
        assert!(builtin("MacBook Pro Microphone"));
        assert!(builtin("MacBook Air Microphone"));
        assert!(builtin("iMac Microphone"));
        assert!(builtin("Studio Display Microphone"));
        assert!(builtin("Built-in Microphone"));
    }

    #[test]
    fn linux_onboard_inputs_are_recognised() {
        assert!(builtin("alsa_input.pci-0000_00_1f.3.analog-stereo"));
        assert!(builtin("Built-in Audio Analog Stereo"));
        assert!(builtin("Internal Mic"));
        assert!(builtin("Digital Microphone"));
    }

    /// Interfaces — including the ones the rigs actually use — pass.
    #[test]
    fn audio_interfaces_are_not_built_in() {
        for name in [
            "MiniFuse 4",
            "MiniFuse 4 Mic1",
            "alsa_input.usb-Arturia_MiniFuse_4-00.analog-surround-40",
            "Yamaha TF",
            "Scarlett 2i2 USB",
            "Decibel Monitoring",
            "MacBook Pro Speakers",
        ] {
            assert!(!builtin(name), "{name} misread as a built-in mic");
        }
    }
}
