//! Electric keys group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Electric keys group (Rhodes, Wurlitzer, DX7, Mellotron, etc.)
pub struct ElectricKeys;

impl From<ElectricKeys> for Group<ItemMetadata> {
    fn from(_val: ElectricKeys) -> Self {
        Self::builder("Electric")
            .patterns(vec![
                "electric_piano",
                "electric piano",
                "elec_piano",
                "elecpiano",
                "elec piano",
                "ep",
                "e_piano",
                "e piano",
            ])
            // Add specific instrument subgroups
            .group(Rhodes)
            .group(Wurlitzer)
            .group(DX7)
            .group(Mellotron)
            .group(Clavinet)
            .group(Pianet)
            .build()
    }
}

/// Rhodes electric piano
pub struct Rhodes;

impl From<Rhodes> for Group<ItemMetadata> {
    fn from(_val: Rhodes) -> Self {
        Self::builder("Rhodes")
            .patterns(vec!["rhodes", "fender_rhodes", "fender rhodes"])
            .build()
    }
}

/// Wurlitzer electric piano
pub struct Wurlitzer;

impl From<Wurlitzer> for Group<ItemMetadata> {
    fn from(_val: Wurlitzer) -> Self {
        Self::builder("Wurlitzer")
            .patterns(vec!["wurlitzer", "wurli", "wurly"])
            .build()
    }
}

/// DX7 FM synthesizer
pub struct DX7;

impl From<DX7> for Group<ItemMetadata> {
    fn from(_val: DX7) -> Self {
        Self::builder("DX7")
            .patterns(vec!["dx7", "dx-7", "dx 7", "fm piano", "fm_piano"])
            .build()
    }
}

/// Mellotron tape-based keyboard
pub struct Mellotron;

impl From<Mellotron> for Group<ItemMetadata> {
    fn from(_val: Mellotron) -> Self {
        Self::builder("Mellotron")
            .patterns(vec!["mellotron", "chamberlin"])
            .build()
    }
}

/// Clavinet
pub struct Clavinet;

impl From<Clavinet> for Group<ItemMetadata> {
    fn from(_val: Clavinet) -> Self {
        Self::builder("Clavinet")
            .patterns(vec!["clav", "clavinet"])
            .build()
    }
}

/// Hohner Pianet
pub struct Pianet;

impl From<Pianet> for Group<ItemMetadata> {
    fn from(_val: Pianet) -> Self {
        Self::builder("Pianet")
            .patterns(vec!["pianet", "hohner"])
            .build()
    }
}
