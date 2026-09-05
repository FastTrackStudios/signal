//! Catalog payloads → the wire contract.
//!
//! Kept apart from the service because these are the only parts of the
//! integration that can be tested without a network or a session, and because
//! the two shapes disagree in ways worth stating once: a searched tone and a
//! fetched tone are different payloads upstream (licence, links and sizes are
//! detail-only), so a row is mapped from what a row actually has.

use signal_tone3000_proto::{PickedTone, TonePage, ToneModel, ToneSummary};
use tone3000::{Model, Page, Tone};

/// One tone as a list row.
#[must_use]
pub fn summary(tone: &Tone) -> ToneSummary {
    ToneSummary {
        id: tone.id.to_string(),
        title: tone.title.clone(),
        creator: creator_name(tone),
        gear: tone.gear.as_ref().map(|g| g.as_str().to_string()).unwrap_or_default(),
        format: tone.format.as_ref().map(|f| f.as_str().to_string()).unwrap_or_default(),
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
    TonePage { error: error.to_string(), ..TonePage::default() }
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
        gear: tone.gear.as_ref().map(|g| g.as_str().to_string()).unwrap_or_default(),
        makes: tone.makes.iter().map(|m| m.name.clone()).collect(),
        tags: tone.tags.iter().map(|t| t.name.clone()).collect(),
        images: tone.images.clone(),
        error: String::new(),
    }
}

/// A tone that could not be fetched.
#[must_use]
pub fn failed_tone(error: impl std::fmt::Display) -> PickedTone {
    PickedTone { error: error.to_string(), ..PickedTone::default() }
}

/// One downloadable variant.
#[must_use]
pub fn model(m: &Model) -> ToneModel {
    ToneModel {
        id: m.id.to_string(),
        name: m.name.clone(),
        size: m.size.as_ref().map(|s| s.as_str().to_string()).unwrap_or_default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_json() -> Tone {
        serde_json::from_str(
            r#"{
                "id": 51949, "user_id": "57af", "title": "Plexi 51",
                "description": "1968 Super Lead, SM57",
                "gear": "amp", "license": "cc-by", "format": "nam",
                "makes": [{"name": "Marshall Plexi"}], "tags": [{"name": "crunch"}],
                "images": ["https://cdn.example/a.jpg", "https://cdn.example/b.jpg"],
                "user": {"id": "57af", "username": "brucew", "url": "https://t/u/brucew"},
                "url": "https://www.tone3000.com/tones/51949",
                "models_count": 6, "a1_models_count": 3, "a2_models_count": 3,
                "downloads_count": 900, "favorites_count": 12
            }"#,
        )
        .expect("fixture parses")
    }

    #[test]
    fn a_row_carries_what_a_row_is_given() {
        let row = summary(&tone_json());
        assert_eq!(row.id, "51949");
        assert_eq!(row.title, "Plexi 51");
        assert_eq!(row.creator, "brucew");
        assert_eq!(row.gear, "amp");
        assert_eq!(row.format, "nam");
        assert_eq!(row.makes, vec!["Marshall Plexi".to_string()]);
        assert_eq!(row.image, "https://cdn.example/a.jpg");
        assert_eq!(row.downloads_count, 900);
    }

    /// The obligation the API terms put on us: creator, licence and the tone's
    /// own page must survive into anything we keep.
    #[test]
    fn attribution_survives_the_mapping() {
        let full = picked(&tone_json(), &[]);
        assert_eq!(full.creator, "brucew");
        assert_eq!(full.creator_url, "https://t/u/brucew");
        assert_eq!(full.license, "cc-by");
        assert_eq!(full.tone_url, "https://www.tone3000.com/tones/51949");
        assert_eq!(full.images.len(), 2, "every image, not just the first");
    }

    /// Search results omit licence and description entirely. Mapping must not
    /// invent them — an empty licence means "not stated here", and a UI that
    /// needs one calls for the detail.
    #[test]
    fn a_sparse_payload_maps_to_empty_not_to_a_guess() {
        let sparse: Tone =
            serde_json::from_str(r#"{"id": 1, "user_id": "u"}"#).expect("sparse tone parses");
        let row = summary(&sparse);
        assert_eq!(row.creator, "");
        assert_eq!(row.gear, "");
        assert_eq!(row.image, "");
        let full = picked(&sparse, &[]);
        assert_eq!(full.license, "");
        assert_eq!(full.description, "");
        assert!(full.error.is_empty(), "a sparse tone is not a failed one");
    }

    #[test]
    fn models_carry_their_size_and_architecture() {
        let m: Model = serde_json::from_str(
            r#"{"id": 293886, "tone_id": 51949, "user_id": "57af",
                "name": "Plexi 51 DI#03",
                "model_url": "https://x/models/293886/download/a.nam",
                "size": "standard", "architecture_version": "2"}"#,
        )
        .expect("model fixture parses");
        let mapped = model(&m);
        assert_eq!(mapped.id, "293886");
        assert_eq!(mapped.size, "standard");
        assert_eq!(mapped.architecture, "2");
    }

    #[test]
    fn a_failed_page_is_distinguishable_from_an_empty_one() {
        let failed = failed_page("rate limited");
        assert!(failed.tones.is_empty());
        assert_eq!(failed.error, "rate limited");
        assert!(TonePage::default().error.is_empty());
    }
}
