//! A double, aligned to its lead: the golden session's DBL sings 80 ms
//! behind Main, wandering (`golden_session::perform::double_lag`), and
//! aligned over the stretch the two share, each note lands back on its
//! lead's.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use std::path::Path;

use session_daw::studio::StudioSession;

#[test]
fn aligned_the_double_lands_on_its_lead() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp");
    session_daw::open::open_silent(&path).expect("the golden session opens");
    let session = StudioSession::read_current(Some(&path), None).expect("and reads back");
    let project = &session.project.0;
    // Ron's Main and DBL, in English: the first item of each.
    let take = |which: &str| {
        let parent = project
            .tracks
            .iter()
            .find(|t| {
                t.name == which
                    && project
                        .tracks
                        .iter()
                        .any(|p| Some(&p.guid) == t.parent_guid.as_ref() && p.name == "Ron")
            })
            .expect("the take's folder");
        let en = project
            .tracks
            .iter()
            .find(|t| t.parent_guid.as_ref() == Some(&parent.guid) && t.name == "EN")
            .expect("its English");
        project.items[&en.guid].clone()
    };
    // A Main item and the DBL item that overlaps it the most: the two
    // sing the same stretch of the song, each from its own start.
    let span = |i: &daw_proto::Item| {
        let s = i.position.as_seconds();
        (s, s + i.length.as_seconds())
    };
    let overlap = |a: &daw_proto::Item, b: &daw_proto::Item| {
        let ((a0, a1), (b0, b1)) = (span(a), span(b));
        (a1.min(b1) - a0.max(b0)).max(0.0)
    };
    let (main, dbl) = take("Main")
        .into_iter()
        .flat_map(|m| take("DBL").into_iter().map(move |d| (m.clone(), d)))
        .max_by(|a, b| overlap(&a.0, &a.1).total_cmp(&overlap(&b.0, &b.1)))
        .expect("a pair");
    assert!(overlap(&main, &dbl) > 8.0, "they share a stretch");

    let analysed = |item: &daw_proto::Item| {
        session_daw::editor_view::analysis(&item.guid, item.length.as_seconds(), item.volume)
            .expect("the take analyses")
    };
    let (reference, dub) = (analysed(&main), analysed(&dbl));
    let alignment = expression_editor_audio::align_placed(
        &reference,
        main.position.as_seconds(),
        &dub,
        dbl.position.as_seconds(),
        expression_editor_audio::AlignConfig::default(),
    )
    .expect("an alignment");
    // The truth, from the generator: each note the lead begins at `on`,
    // the double begins `double_lag(on)` later. Aligned, every note of the
    // double inside the shared stretch lands back on its lead's — within
    // an analysis frame (these files run at 11 kHz: 46 ms) — where before
    // it was 55–105 ms off.
    use dynamic_template::golden_session::{perform, shape};
    let rate = alignment.frame_rate;
    let frame = 1.0 / rate;
    let (from, to) = (
        main.position.as_seconds().max(dbl.position.as_seconds()) + 0.5,
        (main.position.as_seconds() + main.length.as_seconds())
            .min(dbl.position.as_seconds() + dbl.length.as_seconds())
            - 0.5,
    );
    let dbl_start = dbl.position.as_seconds();
    let (mut before, mut after) = (Vec::new(), Vec::new());
    for on in perform::melody_onsets(&shape::maximal()) {
        if on < from || on > to {
            continue;
        }
        let sung = on + perform::double_lag(on) - dbl_start;
        let lands = expression_editor_audio::align::warped(&alignment, sung);
        before.push((sung - (on - dbl_start)).abs());
        after.push((lands - (on - dbl_start)).abs());
    }
    assert!(before.len() > 8, "notes in the stretch: {}", before.len());
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let worst = after.iter().copied().fold(0.0, f64::max);
    assert!(
        mean(&after) < 0.5 * frame && worst < frame,
        "aligned, the double lands on its lead: mean {:.1} ms, worst {:.1} ms \
         (before: mean {:.1} ms; a frame is {:.1} ms)",
        mean(&after) * 1e3,
        worst * 1e3,
        mean(&before) * 1e3,
        frame * 1e3
    );
    // Negative control: the lead against itself moves nowhere.
    let itself = expression_editor_audio::align(
        &reference,
        &reference,
        expression_editor_audio::AlignConfig::default(),
    )
    .expect("an alignment");
    assert!(
        itself.offset.seconds.abs() < 0.005,
        "{}",
        itself.offset.seconds
    );
}
