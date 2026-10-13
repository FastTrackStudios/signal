//! Round-trip test: ensures the new tag/instrument/category/style fields
//! parse cleanly via facet-styx.

use signal_sampler::LibrarySpec;

#[test]
fn parses_pack_metadata_fields() {
    let s = r#"name "test"
instrument "violin"
category "orchestral"
style ("legato" "vibrato")
"#;
    let spec = LibrarySpec::from_styx(s).expect("parse styx");
    assert_eq!(spec.instrument, "violin");
    assert_eq!(spec.category, "orchestral");
    assert_eq!(
        spec.style,
        vec!["legato".to_string(), "vibrato".to_string()]
    );
}

#[test]
fn parses_tagset_field() {
    let s = r#"name "test"
tags ({
  category @Tone
  value "clean"
  source @Manual
  weight 7
})
"#;
    let spec = LibrarySpec::from_styx(s).expect("parse styx with tags");
    assert_eq!(spec.tags.len(), 1);
    let set = spec.tag_set();
    assert_eq!(set.values().count(), 1);
}

#[test]
fn metadata_fields_are_optional() {
    let s = r#"name "minimal"
"#;
    let spec = LibrarySpec::from_styx(s).expect("parse minimal");
    assert_eq!(spec.instrument, "");
    assert_eq!(spec.category, "");
    assert!(spec.style.is_empty());
    assert!(spec.tags.is_empty());
}
