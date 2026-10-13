//! Percussion group definitions

use crate::item_metadata::{ItemMetadata, ItemMetadataGroupExt};
use monarchy::Group;

/// Top-level percussion group for non-drum kit percussion instruments
pub struct Percussion;

impl From<Percussion> for Group<ItemMetadata> {
    fn from(_val: Percussion) -> Self {
        Self::builder("Percussion")
            .prefix("Perc")
            .patterns(vec![
                "percussion",
                "perc",
                "aux perc",
                "auxperc",
                "aux_perc",
            ])
            // Exclude drum kit items
            .exclude(vec!["drum", "kick", "snare", "hihat", "cymbal", "tom"])
            // Add subgroups for specific percussion instruments
            .group(Shaker)
            .group(Tambourine)
            .group(Cabasa)
            .group(Guiro)
            .group(Clave)
            .group(Vibraslap)
            .group(Conga)
            .group(Bongo)
            .group(Cowbell)
            .group(Woodblock)
            .group(Clap)
            .group(Triangle)
            .group(Maracas)
            .group(Cajon)
            .group(Djembe)
            .group(Timbales)
            .group(Chimes)
            .group(Agogo)
            .group(Castanets)
            .group(Sidestick)
            .group(Rimshot)
            .group(Tabla)
            .group(Darbuka)
            .build()
    }
}

pub struct Sidestick;
impl From<Sidestick> for Group<ItemMetadata> {
    fn from(_val: Sidestick) -> Self {
        Self::builder("Sidestick")
            .patterns(vec![
                "sidestick",
                "side stick",
                "side-stick",
                "xstick",
                "x-stick",
            ])
            .build()
    }
}

pub struct Rimshot;
impl From<Rimshot> for Group<ItemMetadata> {
    fn from(_val: Rimshot) -> Self {
        Self::builder("Rimshot")
            .patterns(vec!["rimshot", "rim shot", "rim-shot", "rim"])
            .build()
    }
}

// Individual percussion instrument groups

pub struct Shaker;
impl From<Shaker> for Group<ItemMetadata> {
    fn from(_val: Shaker) -> Self {
        Self::builder("Shaker").patterns(vec!["shaker"]).build()
    }
}

pub struct Tambourine;
impl From<Tambourine> for Group<ItemMetadata> {
    fn from(_val: Tambourine) -> Self {
        Self::builder("Tambourine")
            .patterns(vec!["tambourine", "tamb", "tambo"])
            .build()
    }
}

pub struct Cabasa;
impl From<Cabasa> for Group<ItemMetadata> {
    fn from(_val: Cabasa) -> Self {
        Self::builder("Cabasa").patterns(vec!["cabasa"]).build()
    }
}

pub struct Guiro;
impl From<Guiro> for Group<ItemMetadata> {
    fn from(_val: Guiro) -> Self {
        use crate::item_metadata::ItemMetadataField;

        // Guiro can be played with different techniques/implements
        // "Guiro Shaker" = guiro played with shaker-like technique
        // "Guiro" alone defaults to "Main" arrangement
        let technique = Self::builder("Arrangement")
            .patterns(vec!["shaker", "stick", "mallet", "brush"])
            .build();

        Self::builder("Guiro")
            .patterns(vec!["guiro"])
            .arrangement(technique)
            .field_default_value(ItemMetadataField::Arrangement, "Main")
            .build()
    }
}

pub struct Clave;
impl From<Clave> for Group<ItemMetadata> {
    fn from(_val: Clave) -> Self {
        Self::builder("Clave").patterns(vec!["clave"]).build()
    }
}

pub struct Vibraslap;
impl From<Vibraslap> for Group<ItemMetadata> {
    fn from(_val: Vibraslap) -> Self {
        Self::builder("Vibraslap")
            .patterns(vec!["vibraslap"])
            .build()
    }
}

pub struct Conga;
impl From<Conga> for Group<ItemMetadata> {
    fn from(_val: Conga) -> Self {
        Self::builder("Conga")
            .patterns(vec!["conga", "congas"])
            .build()
    }
}

pub struct Bongo;
impl From<Bongo> for Group<ItemMetadata> {
    fn from(_val: Bongo) -> Self {
        Self::builder("Bongo").patterns(vec!["bongo"]).build()
    }
}

pub struct Cowbell;
impl From<Cowbell> for Group<ItemMetadata> {
    fn from(_val: Cowbell) -> Self {
        Self::builder("Cowbell").patterns(vec!["cowbell"]).build()
    }
}

pub struct Woodblock;
impl From<Woodblock> for Group<ItemMetadata> {
    fn from(_val: Woodblock) -> Self {
        Self::builder("Woodblock")
            .patterns(vec!["woodblock", "wood block"])
            .build()
    }
}

pub struct Clap;
impl From<Clap> for Group<ItemMetadata> {
    fn from(_val: Clap) -> Self {
        Self::builder("Clap")
            .patterns(vec!["clap", "claps", "handclap", "handclaps"])
            .build()
    }
}

pub struct Triangle;
impl From<Triangle> for Group<ItemMetadata> {
    fn from(_val: Triangle) -> Self {
        Self::builder("Triangle").patterns(vec!["triangle"]).build()
    }
}

pub struct Maracas;
impl From<Maracas> for Group<ItemMetadata> {
    fn from(_val: Maracas) -> Self {
        Self::builder("Maracas")
            .patterns(vec!["maracas", "maraca"])
            .build()
    }
}

pub struct Cajon;
impl From<Cajon> for Group<ItemMetadata> {
    fn from(_val: Cajon) -> Self {
        Self::builder("Cajon").patterns(vec!["cajon"]).build()
    }
}

pub struct Djembe;
impl From<Djembe> for Group<ItemMetadata> {
    fn from(_val: Djembe) -> Self {
        Self::builder("Djembe").patterns(vec!["djembe"]).build()
    }
}

pub struct Timbales;
impl From<Timbales> for Group<ItemMetadata> {
    fn from(_val: Timbales) -> Self {
        Self::builder("Timbales")
            .patterns(vec!["timbales", "timbale"])
            .build()
    }
}

pub struct Chimes;
impl From<Chimes> for Group<ItemMetadata> {
    fn from(_val: Chimes) -> Self {
        Self::builder("Chimes")
            .patterns(vec!["chimes", "chime"])
            .build()
    }
}

pub struct Agogo;
impl From<Agogo> for Group<ItemMetadata> {
    fn from(_val: Agogo) -> Self {
        Self::builder("Agogo").patterns(vec!["agogo"]).build()
    }
}

pub struct Castanets;
impl From<Castanets> for Group<ItemMetadata> {
    fn from(_val: Castanets) -> Self {
        Self::builder("Castanets")
            .patterns(vec!["castanets", "castanet"])
            .build()
    }
}

pub struct Tabla;
impl From<Tabla> for Group<ItemMetadata> {
    fn from(_val: Tabla) -> Self {
        Self::builder("Tabla").patterns(vec!["tabla"]).build()
    }
}

pub struct Darbuka;
impl From<Darbuka> for Group<ItemMetadata> {
    fn from(_val: Darbuka) -> Self {
        Self::builder("Darbuka")
            .patterns(vec!["darbuka", "doumbek", "dumbek", "darabuka"])
            .build()
    }
}
