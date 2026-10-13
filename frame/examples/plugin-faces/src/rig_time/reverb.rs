//! The reverb algorithms' faces, in the block's `algorithm` order.

mod room;
pub(crate) mod hall;
pub(crate) mod plate;
mod spring;
pub(crate) mod cloud;
mod bloom;
mod shimmer;
mod chorale;
mod magneto;
mod nonlinear;
mod swell;
mod reflections;
mod velvet;
mod freeverb;
mod convolution;
mod random;
mod shell;
/// The 480: a machine, not an algorithm — on any reverb, by name.
pub mod four80;

use crate::face::Face;

/// Every reverb face, in `kit::VERB_ALGOS` order.
#[must_use]
/// The phone layouts there are, by algorithm (its index in
/// `VERB_ALGOS`'s order, as [`faces`] lists them).
pub fn phone_faces(dark: bool) -> Vec<(usize, Face)> {
    small_faces(dark, super::kit::Tier::Phone)
}

/// The layouts there are for a desktop pane's lane, by algorithm.
pub fn pane_faces(dark: bool) -> Vec<(usize, Face)> {
    small_faces(dark, super::kit::Tier::Ipad)
}

/// The small-room layouts at `t` (a phone's lane, a desktop pane's), by
/// algorithm: every machine has its own now.
pub fn small_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(0, room::face_at(dark, t)), (1, hall::face_at(dark, t)), (2, plate::face_at(dark, t)), (3, spring::face_at(dark, t)), (4, cloud::face_at(dark, t)), (5, bloom::face_at(dark, t)), (6, shimmer::face_at(dark, t)), (7, chorale::face_at(dark, t)), (8, magneto::face_at(dark, t)), (9, nonlinear::face_at(dark, t)), (10, swell::face_at(dark, t)), (11, reflections::face_at(dark, t)), (12, velvet::face_at(dark, t)), (13, freeverb::face_at(dark, t)), (14, convolution::face_at(dark, t)), (15, random::face_at(dark, t))]
}

/// The backs there are (the advanced controls), by algorithm, at `t`.
pub fn back_faces(dark: bool, t: super::kit::Tier) -> Vec<(usize, Face)> {
    vec![(0, room::back_at(dark, t)), (1, hall::back_at(dark, t)), (2, plate::back_at(dark, t)), (3, spring::back_at(dark, t)), (4, cloud::back_at(dark, t)), (5, bloom::back_at(dark, t)), (6, shimmer::back_at(dark, t)), (7, chorale::back_at(dark, t)), (8, magneto::back_at(dark, t)), (9, nonlinear::back_at(dark, t)), (10, swell::back_at(dark, t)), (11, reflections::back_at(dark, t)), (12, velvet::back_at(dark, t)), (13, freeverb::back_at(dark, t)), (14, convolution::back_at(dark, t)), (15, random::back_at(dark, t))]
}

/// Every reverb's tall face (see `kit::tall_chrome`), in `VERB_ALGOS`
/// order: its own where it has one, else the placeholder's.
#[must_use]
pub fn tall_faces(dark: bool) -> Vec<Face> {
    faces(false)
        .into_iter()
        .zip(super::kit::VERB_ALGOS)
        .map(|(f, name)| match name {
            "Room" => room::tall(dark),
            "Hall" => hall::tall(dark),
            "Plate" => plate::tall(dark),
            "Spring" => spring::tall(dark),
            "Cloud" => cloud::tall(dark),
            "Bloom" => bloom::tall(dark),
            "Shimmer" => shimmer::tall(dark),
            "Chorale" => chorale::tall(dark),
            "Magneto" => magneto::tall(dark),
            "NonLinear" => nonlinear::tall(dark),
            "Random" => random::tall(dark),
            "Swell" => swell::tall(dark),
            "Reflections" => reflections::tall(dark),
            "Velvet" => velvet::tall(dark),
            "FreeVerb" => freeverb::tall(dark),
            "Convolution" => convolution::tall(dark),
            _ => super::kit::stub_tall(super::kit::VERB, name, f.file, dark),
        })
        .collect()
}

pub fn faces(dark: bool) -> Vec<Face> {
    vec![room::face(dark), hall::face(dark), plate::face(dark), spring::face(dark), cloud::face(dark), bloom::face(dark), shimmer::face(dark), chorale::face(dark), magneto::face(dark), nonlinear::face(dark), swell::face(dark), reflections::face(dark), velvet::face(dark), freeverb::face(dark), convolution::face(dark), random::face(dark)]
}
