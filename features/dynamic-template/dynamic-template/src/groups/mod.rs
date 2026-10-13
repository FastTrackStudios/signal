//! Group definitions for the dynamic template system

pub mod bass;
pub mod choir;
pub mod drums;
pub mod guide;
pub mod guitars;
pub mod harmonica;
pub mod headphones;
pub mod horns;
pub mod keys;
pub mod orchestra;
pub mod percussion;
pub mod reference;
pub mod sfx;
pub mod stem_split;
pub mod strings;
pub mod synths;
pub mod talkback;
pub mod tracks;
pub mod vca;
pub mod vocals;

// Re-export the top-level containers
// Individual types should be accessed through their respective modules
// e.g., drums::drum_kit::Kick, bass::bass_guitar::BassGuitar, guitars::electric_guitar::ElectricGuitar
pub use bass::Bass;
pub use choir::Choir;
pub use drums::Drums;
pub use guide::Guide;
pub use guitars::Guitars;
pub use harmonica::Harmonica;
pub use headphones::Headphones;
pub use horns::Horns;
pub use keys::Keys;
pub use orchestra::Orchestra;
pub use percussion::Percussion;
pub use reference::Reference;
pub use sfx::SFX;
pub use stem_split::StemSplit;
pub use strings::Strings;
pub use synths::Synths;
pub use talkback::Talkback;
pub use tracks::Tracks;
pub use vca::Vca;
pub use vocals::Vocals;
