//! Reverb algorithm implementations.

pub mod bloom;
pub mod chorale;
pub mod cloud;
pub mod convolution;
pub mod freeverb;
pub mod hall;
pub mod hall_arena;
pub mod hall_cathedral;
pub mod magneto;
pub mod nonlinear;
pub mod plate;
pub mod plate_lexicon;
pub mod plate_progenitor;
pub mod reflections;
pub mod room;
pub mod room_chamber;
pub mod room_studio;
pub mod shimmer;
pub mod spring;
pub mod spring_vintage;
pub mod swell;
pub mod velvet;

use crate::algorithm::{AlgorithmType, ReverbAlgorithm};

/// Create a reverb algorithm instance for the given type and variant.
pub fn create(
    algorithm: AlgorithmType,
    variant: usize,
    sample_rate: f64,
) -> Box<dyn ReverbAlgorithm> {
    match algorithm {
        AlgorithmType::Room => match variant {
            1 => Box::new(room_chamber::RoomChamber::new(sample_rate)),
            2 => Box::new(room_studio::RoomStudio::new(sample_rate)),
            _ => Box::new(room::Room::new(sample_rate)),
        },
        AlgorithmType::Hall => match variant {
            1 => Box::new(hall_cathedral::HallCathedral::new(sample_rate)),
            2 => Box::new(hall_arena::HallArena::new(sample_rate)),
            _ => Box::new(hall::Hall::new(sample_rate)),
        },
        AlgorithmType::Plate => match variant {
            1 => Box::new(plate_lexicon::PlateLexicon::new(sample_rate)),
            2 => Box::new(plate_progenitor::PlateProgenitor::new(sample_rate)),
            _ => Box::new(plate::Plate::new(sample_rate)),
        },
        AlgorithmType::Spring => match variant {
            1 => Box::new(spring_vintage::SpringVintage::new(sample_rate)),
            _ => Box::new(spring::Spring::new(sample_rate)),
        },
        AlgorithmType::Cloud => Box::new(cloud::Cloud::new(sample_rate)),
        AlgorithmType::Bloom => Box::new(bloom::Bloom::new(sample_rate)),
        AlgorithmType::Shimmer => Box::new(shimmer::Shimmer::new(sample_rate)),
        AlgorithmType::Chorale => Box::new(chorale::Chorale::new(sample_rate)),
        AlgorithmType::Magneto => Box::new(magneto::Magneto::new(sample_rate)),
        AlgorithmType::NonLinear => Box::new(nonlinear::NonLinear::new(sample_rate)),
        AlgorithmType::Swell => Box::new(swell::Swell::new(sample_rate)),
        AlgorithmType::Reflections => Box::new(reflections::Reflections::new(sample_rate)),
        AlgorithmType::Velvet => Box::new(velvet::Velvet::new(sample_rate)),
        AlgorithmType::FreeVerb => Box::new(freeverb::FreeVerb::new(sample_rate)),
        AlgorithmType::Convolution => Box::new(convolution::Convolution::new(sample_rate)),
    }
}
