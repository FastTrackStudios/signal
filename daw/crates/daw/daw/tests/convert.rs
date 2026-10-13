//! Integration test for `daw::file::convert`. Run with `--features convert`.
#![cfg(feature = "convert")]

#[test]
fn ptx_to_rpp() {
    // A real PT fixture from the dawfile-protools crate.
    let input = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../features/dawfile/dawfile-protools/tests/fixtures/HeyLady.ptx"
    );
    let out = std::env::temp_dir().join("daw_convert_test_heylady.rpp");
    daw::file::convert(input, &out).expect("convert ptx -> rpp");

    let rpp = std::fs::read_to_string(&out).unwrap();
    assert!(
        rpp.starts_with("<REAPER_PROJECT"),
        "not an RPP: {}",
        &rpp[..rpp.len().min(40)]
    );
    assert!(rpp.contains("<TRACK"), "no tracks emitted");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn unsupported_output_errors() {
    let input = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../features/dawfile/dawfile-protools/tests/fixtures/HeyLady.ptx"
    );
    let err = daw::file::convert(input, "out.als").unwrap_err();
    assert!(matches!(err, daw::file::ConvertError::UnsupportedOutput(_)));
}
