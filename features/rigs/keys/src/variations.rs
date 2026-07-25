//! **Variations** — the alternatives behind a library default.
//!
//! A library entry is one instrument with several ways to sit in a mix. "LA
//! Custom C7 Grand" is the sound; Rock, Cinematic and Ballad are that same
//! piano voiced for the job. They share the soundsource — the same samples
//! load either way — and differ in what the module does with it: filter,
//! envelopes, tone, ambience.
//!
//! Only the names exist today. Loading a variation loads the default's
//! soundsource and records which variation is chosen, so the rig already
//! tracks the state the parameters will hang off; a variation with no
//! parameters simply sounds like the default. When packs carry authored
//! parameter sets, this table becomes their index and
//! [`Variation::macros`] stops being empty.

/// One named alternative on a library default.
#[derive(Clone, Debug, PartialEq)]
pub struct Variation {
    pub name: &'static str,
    /// Module macro overrides (`"filter.cutoff"` → value) applied over the
    /// default's own settings. Empty until authored — see the module doc.
    pub macros: &'static [(&'static str, f32)],
}

const fn v(name: &'static str) -> Variation {
    Variation { name, macros: &[] }
}

/// The C7's set: how a grand gets used, not how it is built.
const GRAND: &[Variation] = &[
    v("Rock"),
    v("Cinematic"),
    v("Softest"),
    v("Bright"),
    v("Pop"),
    v("Indie"),
    v("Stage"),
    v("Ballad"),
];

/// The Rhodes' set: the amp and effects a Rhodes is normally heard through.
const RHODES: &[Variation] = &[v("Chorus"), v("Suitcase"), v("Warm"), v("Lush")];

/// The variations authored on `preset`, by its library name. Matching is by
/// name because that is what the library scan has — a pack-level id lands here
/// when packs carry one.
pub fn variations_for(preset: &str) -> &'static [Variation] {
    let n = preset.to_ascii_lowercase();
    if n.contains("c7 grand") || (n.contains("c7") && n.contains("grand")) {
        GRAND
    } else if n.contains("rhodes") && n.contains("la custom") {
        RHODES
    } else {
        &[]
    }
}

/// Just the names, for the wire.
pub fn variation_names(preset: &str) -> Vec<String> {
    variations_for(preset).iter().map(|v| v.name.to_string()).collect()
}
