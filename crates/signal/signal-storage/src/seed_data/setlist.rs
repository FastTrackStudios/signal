//! Setlist seed data — demo ordered song lists.

use signal_proto::metadata::Metadata;
use signal_proto::seed_id;
use signal_proto::setlist::{Setlist, SetlistEntry};

/// All default setlist collections.
pub fn setlists() -> Vec<Setlist> {
    vec![worship_set(), commercial_music(), night_of_entertainment()]
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

    let mut setlist = Setlist::new(seed_id("worship-set"), "Worship Set", worship).with_metadata(
        Metadata::new()
            .with_tag("worship")
            .with_tag("setlist")
            .with_description("Sunday worship setlist — guitar and keys"),
    );
    setlist.add_entry(keys_feature);
    setlist
}

fn commercial_music() -> Setlist {
    let four_am = SetlistEntry::new(
        seed_id("commercial-four-am"),
        "4 A.M.",
        seed_id("four-am-song"),
    );

    let thriller = SetlistEntry::new(
        seed_id("commercial-thriller"),
        "Thriller",
        seed_id("thriller-song"),
    );

    let movin_out = SetlistEntry::new(
        seed_id("commercial-movin-out"),
        "Movin' Out",
        seed_id("movin-out-song"),
    );

    let girl_goodbye = SetlistEntry::new(
        seed_id("commercial-girl-goodbye"),
        "Girl Goodbye",
        seed_id("girl-goodbye-song"),
    );

    let bennie = SetlistEntry::new(
        seed_id("commercial-bennie-jets"),
        "Bennie And The Jets",
        seed_id("bennie-jets-song"),
    );

    let cryin = SetlistEntry::new(
        seed_id("commercial-cryin-out-loud"),
        "For Cryin' Out Loud",
        seed_id("cryin-out-loud-song"),
    );

    let never_friends = SetlistEntry::new(
        seed_id("commercial-never-friends"),
        "We Were Never Really Friends",
        seed_id("never-friends-song"),
    );

    let dont_trust = SetlistEntry::new(
        seed_id("commercial-dont-trust"),
        "I Don't Trust Myself",
        seed_id("dont-trust-song"),
    );

    let mut setlist = Setlist::new(seed_id("commercial-music"), "Commercial Music", four_am)
        .with_metadata(
            Metadata::new()
                .with_tag("commercial")
                .with_tag("setlist")
                .with_description("Commercial gig setlist — guitar songs with base profiles"),
        );
    setlist.add_entry(thriller);
    setlist.add_entry(movin_out);
    setlist.add_entry(girl_goodbye);
    setlist.add_entry(bennie);
    setlist.add_entry(cryin);
    setlist.add_entry(never_friends);
    setlist.add_entry(dont_trust);
    setlist
}

fn night_of_entertainment() -> Setlist {
    let come_and_see = SetlistEntry::new(
        seed_id("noe-come-see"),
        "Come and See",
        seed_id("come-see-song"),
    );

    let holy_one = SetlistEntry::new(
        seed_id("noe-holy-one"),
        "Holy One",
        seed_id("holy-one-song"),
    );

    let separate_ways = SetlistEntry::new(
        seed_id("noe-separate-ways"),
        "Separate Ways",
        seed_id("separate-ways-song"),
    );

    let mut setlist = Setlist::new(
        seed_id("night-of-entertainment"),
        "Night of Entertainment",
        come_and_see,
    )
    .with_metadata(
        Metadata::new()
            .with_tag("entertainment")
            .with_tag("setlist")
            .with_description("Night of Entertainment setlist — worship and rock"),
    );
    setlist.add_entry(holy_one);
    setlist.add_entry(separate_ways);
    setlist
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setlist_count() {
        assert_eq!(setlists().len(), 3);
    }

    #[test]
    fn worship_set_contains_two_entries() {
        let setlist = setlists()
            .into_iter()
            .find(|s| s.name == "Worship Set")
            .unwrap();
        assert_eq!(setlist.entries.len(), 2);
    }

    #[test]
    fn commercial_music_contains_eight_entries() {
        let setlist = setlists()
            .into_iter()
            .find(|s| s.name == "Commercial Music")
            .unwrap();
        assert_eq!(setlist.entries.len(), 8);
        let names: Vec<&str> = setlist.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "4 A.M.",
                "Thriller",
                "Movin' Out",
                "Girl Goodbye",
                "Bennie And The Jets",
                "For Cryin' Out Loud",
                "We Were Never Really Friends",
                "I Don't Trust Myself",
            ]
        );
    }
}
