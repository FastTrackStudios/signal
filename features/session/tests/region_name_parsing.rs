//! What `parse_region_section_type` must make of the region names real
//! sessions actually carry.
//!
//! Built from a survey of every distinct marker/region name in the Crescendum
//! (Rockstars) album — 58 of them — rather than from what a tidy naming
//! convention would look like. The canonical vocabulary already handled
//! `pre-chorus`, `Pre- Chorus`, `CH 1`, `Solo {GTR}` and `OUT 2`; it did not
//! handle `pre chorus`, the plain space-separated form and the most common
//! section name in the album after `chorus` itself. Those regions were
//! silently dropped from every setlist built from these projects.
//!
//! The negative cases matter as much as the positive ones: an arrangement
//! note like `back to 4/4` or `double kick part` must NOT become a section.
//! It belongs on the Marks lane, and a parser eager enough to claim it would
//! put a fake section in the middle of the song.

use session::keyflow::actions::parse_region_section_type;

fn parsed(name: &str) -> Option<String> {
    parse_region_section_type(name).map(|t| format!("{t:?}"))
}

#[test]
fn spaced_multi_word_sections_parse_as_the_right_section() {
    // The whole point: `pre chorus` is a PRE-chorus, not a chorus. Getting
    // this wrong is invisible — the region still lands, just under the wrong
    // section for the life of the song — which is why it is pinned by name.
    assert_eq!(parsed("pre chorus").as_deref(), Some("Pre(Chorus)"));
    assert_eq!(parsed("post chorus").as_deref(), Some("Post(Chorus)"));
    assert_eq!(parsed("Pre- Chorus").as_deref(), Some("Pre(Chorus)"));
    assert_eq!(parsed("pre-chorus").as_deref(), Some("Pre(Chorus)"));
    assert_eq!(parsed("PRE").as_deref(), Some("Pre(Chorus)"));
}

#[test]
fn a_region_named_for_two_things_takes_the_first() {
    assert_eq!(
        parsed("pre chorus/drum solo").as_deref(),
        Some("Pre(Chorus)")
    );
}

#[test]
fn a_qualified_section_keeps_its_section() {
    assert_eq!(parsed("1st solo").as_deref(), Some("Solo"));
    assert_eq!(parsed("2nd solo").as_deref(), Some("Solo"));
    assert_eq!(parsed("drum solo").as_deref(), Some("Solo"));
}

#[test]
fn the_names_that_already_worked_still_do() {
    for (name, want) in [
        ("chorus", "Chorus"),
        ("CH 1", "Chorus"),
        ("verse", "Verse"),
        ("VS 2", "Verse"),
        ("intro", "Intro"),
        ("IN 2", "Intro"),
        ("outro", "Outro"),
        ("OUT 2", "Outro"),
        ("bridge", "Bridge"),
        ("Interlude {Build}", "Interlude"),
        ("Solo {GTR}", "Solo"),
        ("breakdown", "Breakdown"),
    ] {
        assert_eq!(parsed(name).as_deref(), Some(want), "{name:?}");
    }
}

#[test]
fn arrangement_notes_are_not_sections() {
    // Every one of these is a real marker from the album. They belong on the
    // Marks lane; claiming them as sections would invent structure that isn't
    // in the arrangement.
    for name in [
        "back to 4/4",
        "double kick part",
        "7/4 part",
        "tempo change",
        "DOWN",
        "GTR",
        ">:(",
        ">>:(",
        "A",
        "B",
        "C",
        "D",
        // REAPER's own project-region markers, not song sections.
        "=START",
        "=END",
    ] {
        assert_eq!(parsed(name), None, "{name:?} should not parse as a section");
    }
}

#[test]
fn nothing_parses_out_of_nothing() {
    assert_eq!(parsed(""), None);
    assert_eq!(parsed("   "), None);
    assert_eq!(parsed("/"), None);
}
