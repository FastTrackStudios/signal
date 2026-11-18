//! Catalog payloads → the wire contract.
//!
//! Kept apart from the service because these are the only parts of the
//! integration that can be tested without a network or a session, and because
//! the two shapes disagree in ways worth stating once: a searched tone and a
//! fetched tone are different payloads upstream (licence, links and sizes are
//! detail-only), so a row is mapped from what a row actually has.

use crate::{PickedTone, ToneModel, TonePage, ToneSummary};
use tone3000::{Model, Page, Tone};

/// One tone as a list row.
#[must_use]
pub fn summary(tone: &Tone) -> ToneSummary {
    ToneSummary {
        id: tone.id.to_string(),
        title: tone.title.clone(),
        creator: creator_name(tone),
        gear: tone
            .gear
            .as_ref()
            .map(|g| g.as_str().to_string())
            .unwrap_or_default(),
        format: tone
            .format
            .as_ref()
            .map(|f| f.as_str().to_string())
            .unwrap_or_default(),
        makes: tone.makes.iter().map(|m| m.name.clone()).collect(),
        tags: tone.tags.iter().map(|t| t.name.clone()).collect(),
        image: tone.images.first().cloned().unwrap_or_default(),
        models_count: clamp_u32(tone.models_count),
        downloads_count: clamp_u32(tone.downloads_count),
        favorites_count: clamp_u32(tone.favorites_count),
        tone_url: tone.url.clone(),
    }
}

/// A page of rows, with paging carried through unchanged.
#[must_use]
pub fn page(page: &Page<Tone>) -> TonePage {
    TonePage {
        tones: page.data.iter().map(summary).collect(),
        page: page.page,
        total_pages: page.total_pages,
        total: clamp_u32(page.total),
        error: String::new(),
    }
}

/// A page that failed. The message rides the payload because a UI must be
/// able to tell "nothing matched" from "the request did not happen".
#[must_use]
pub fn failed_page(error: impl std::fmt::Display) -> TonePage {
    TonePage {
        error: error.to_string(),
        ..TonePage::default()
    }
}

/// A tone in full, with the models a caller can actually download.
#[must_use]
pub fn picked(tone: &Tone, models: &[Model]) -> PickedTone {
    PickedTone {
        id: tone.id.to_string(),
        name: tone.title.clone(),
        creator: creator_name(tone),
        creator_url: tone
            .user
            .as_ref()
            .map(|u| u.url.clone())
            .unwrap_or_default(),
        tone_url: tone.url.clone(),
        license: tone
            .license
            .as_ref()
            .map(|l| l.as_str().to_string())
            .unwrap_or_default(),
        models: models.iter().map(model).collect(),
        description: tone.description.clone().unwrap_or_default(),
        gear: tone
            .gear
            .as_ref()
            .map(|g| g.as_str().to_string())
            .unwrap_or_default(),
        makes: tone.makes.iter().map(|m| m.name.clone()).collect(),
        tags: tone.tags.iter().map(|t| t.name.clone()).collect(),
        images: tone.images.clone(),
        error: String::new(),
    }
}

/// A tone that could not be fetched.
#[must_use]
pub fn failed_tone(error: impl std::fmt::Display) -> PickedTone {
    PickedTone {
        error: error.to_string(),
        ..PickedTone::default()
    }
}

/// One downloadable variant.
#[must_use]
pub fn model(m: &Model) -> ToneModel {
    ToneModel {
        id: m.id.to_string(),
        name: m.name.clone(),
        size: m
            .size
            .as_ref()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default(),
        architecture: m
            .architecture_version
            .as_ref()
            .map(|a| a.as_str().to_string())
            .unwrap_or_default(),
    }
}

/// The creator as the catalog names them, or empty when the payload omitted
/// the embedded user. Never a placeholder like "unknown": attribution that
/// invents a name is worse than attribution that admits it has none.
fn creator_name(tone: &Tone) -> String {
    tone.user
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default()
}

/// Counts cross the wire as `u32` — no catalog has four billion of anything,
/// and saturating is the right failure: a number too large to show is shown
/// as the largest we can show, not as zero.
fn clamp_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}
