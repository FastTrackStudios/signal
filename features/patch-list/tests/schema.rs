//! The styx schemas describe the files the Facet types read: the golden
//! fixtures validate against them, and a document the types would
//! reject is rejected by the schema too (the negative control).
//!
//! r[verify flow.patch-list.plan]
//! r[verify flow.patch-list.studio-profiles]

use facet_styx::SchemaFile;

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

use patch_list::{FIXTURE_ALBUM as ALBUM, FIXTURE_STUDIO as ROOM};

const ALBUM_SCHEMA: &str = include_str!("../schema/patch-list.schema.styx");
const ROOM_SCHEMA: &str = include_str!("../schema/studio.schema.styx");

/// Every schema error a document raises, as `Kind at path`.
fn errors(schema: &str, doc: &str) -> std::result::Result<Vec<String>, Box<dyn std::error::Error>> {
    let schema: SchemaFile = facet_styx::from_str(schema)?;
    let doc = styx_tree::parse(doc)?;
    Ok(facet_styx::validate(&doc, &schema)
        .errors
        .iter()
        .map(|e| format!("{:?} at {}", e.kind, e.path))
        .collect())
}

#[test]
fn the_fixture_album_validates_against_the_patch_list_schema() -> Result {
    let errors = errors(ALBUM_SCHEMA, ALBUM)?;
    assert!(errors.is_empty(), "schema errors: {errors:#?}");
    Ok(())
}

#[test]
fn a_bus_without_an_output_fails_the_patch_list_schema() -> Result {
    let errors = errors(ALBUM_SCHEMA, "headphones {\n    cody {for (cody)}\n}")?;
    assert!(
        errors.iter().any(|e| e.contains("MissingField")),
        "expected a missing-field error, got {errors:#?}"
    );
    Ok(())
}

#[test]
fn the_fixture_profile_validates_against_the_studio_schema() -> Result {
    let errors = errors(ROOM_SCHEMA, ROOM)?;
    assert!(errors.is_empty(), "schema errors: {errors:#?}");
    Ok(())
}

#[test]
fn an_input_with_no_channel_fails_the_studio_schema() -> Result {
    let errors = errors(ROOM_SCHEMA, "inputs {\n    \"DI 1\" @audio{}\n}")?;
    assert!(
        errors.iter().any(|e| e.contains("MissingField")),
        "expected a missing-field error, got {errors:#?}"
    );
    Ok(())
}

#[test]
fn an_output_with_one_side_fails_the_studio_schema() -> Result {
    let errors = errors(ROOM_SCHEMA, "outputs {\n    \"HP 1\" {left 0}\n}")?;
    assert!(
        errors.iter().any(|e| e.contains("MissingField")),
        "expected a missing-field error, got {errors:#?}"
    );
    Ok(())
}

#[test]
fn an_unknown_top_level_key_fails_the_patch_list_schema() -> Result {
    let errors = errors(ALBUM_SCHEMA, "performers {}\nkits {}")?;
    assert!(
        errors.iter().any(|e| e.contains("UnknownField")),
        "expected an unknown-field error, got {errors:#?}"
    );
    Ok(())
}
