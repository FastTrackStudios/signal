//! Setlist seed data — demo ordered song lists.

use signal_proto::metadata::Metadata;
use signal_proto::seed_id;
use signal_proto::setlist::{Setlist, SetlistEntry};

/// All default setlist collections.
pub fn setlists() -> Vec<Setlist> {
    vec![worship_set(), commercial_music()]
}

fn worship_set() -> Setlist {
    let worship = SetlistEntry::new(
        seed_id("worship-set-worship-song"),
        "Worship Set",
        seed_id("guitar-worship-song"),
    )
    .with_metadata(Metadata::new().with_tag("guitar"));

    let keys_feature = SetlistEntry::new(
        seed_id("worship-set-keys-feature"),
        "Keys Feature",
        seed_id("feature-demo-song"),
    )
    .with_metadata(Metadata::new().with_tag("keys"));

    let mut setlist =
        Setlist::new(seed_id("worship-set"), "Worship Set", worship).with_metadata(
            Metadata::new()
                .with_tag("worship")
                .with_tag("setlist")
                .with_description("Sunday worship setlist — guitar and keys"),
        );
    setlist.add_entry(keys_feature);
    setlist
}

fn commercial_music() -> Setlist {
    let feature = SetlistEntry::new(
        seed_id("commercial-feature-demo"),
        "Feature-Demo Song",
        seed_id("feature-demo-song"),
    )
    .with_metadata(Metadata::new().with_tag("keys"));

    let dummy = SetlistEntry::new(
        seed_id("commercial-dummy"),
        "Dummy Song",
        seed_id("dummy-song"),
    )
    .with_metadata(Metadata::new().with_tag("dummy"));

    let worship = SetlistEntry::new(
        seed_id("commercial-worship"),
        "Worship Set",
        seed_id("guitar-worship-song"),
    )
    .with_metadata(Metadata::new().with_tag("guitar"));

    let mut setlist =
        Setlist::new(seed_id("commercial-music"), "Commercial Music", feature).with_metadata(
            Metadata::new()
                .with_tag("commercial")
                .with_tag("setlist")
                .with_description("Commercial gig setlist — mixed keys and guitar"),
        );
    setlist.add_entry(dummy);
    setlist.add_entry(worship);
    setlist
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setlist_count() {
        assert_eq!(setlists().len(), 2);
    }

    #[test]
    fn worship_set_contains_two_entries() {
        let setlist = setlists().into_iter().find(|s| s.name == "Worship Set").unwrap();
        assert_eq!(setlist.entries.len(), 2);
    }

    #[test]
    fn commercial_music_contains_three_entries() {
        let setlist = setlists()
            .into_iter()
            .find(|s| s.name == "Commercial Music")
            .unwrap();
        assert_eq!(setlist.entries.len(), 3);
    }
}
