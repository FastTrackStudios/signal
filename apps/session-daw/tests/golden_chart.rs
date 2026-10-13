//! The golden session's chart — `template.kf`, written beside the
//! template from the song its MIDI is (`golden_session::song`) — is a
//! keyflow chart the session reads: its tempo, meter, key and every
//! section, at the template's bars.

use std::path::Path;

#[test]
fn the_golden_chart_is_a_keyflow_chart_of_the_song() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.kf");
    let text = std::fs::read_to_string(&path).expect("the chart is written beside the template");
    let layout = session::setlist::chart_import::chart_to_layout(&text).expect("keyflow reads it");
    assert!((layout.tempo_bpm - 120.0).abs() < 1e-6);
    assert_eq!((layout.time_sig_num, layout.time_sig_den), (4, 4));
    assert_eq!(layout.key.as_deref(), Some("F"));
    // It is the template's song, generated, not a stale copy.
    let shape = dynamic_template::golden_session::shape::maximal();
    let fresh =
        dynamic_template::golden_session::song::chart(shape.name, shape.bpm, &shape.sections);
    assert_eq!(text, fresh, "regenerate with `just daw-template`");
}
