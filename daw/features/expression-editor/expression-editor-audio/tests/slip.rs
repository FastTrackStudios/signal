//! The manual slip against a real host — the standalone backend through
//! the `daw` facade, same as the quantizer's write test.
//!
//! The claims: a slip is the quantizer's split (same pieces on every
//! mic), and only the dragged span moves.

#![cfg(feature = "daw")]

use daw::service::item::Items;
use daw::service::midi::Midi;
use daw::service::{ItemRef, ProjectContext, TakeRef, Takes, TrackRef, Tracks};
use daw::standalone::sync::Standalone;
use expression_editor_audio::quantize::SplitConfig;
use expression_editor_audio::slip::slip_hit;

const TAKE_LEN: f64 = 2.0;

fn project() -> Standalone {
    let daw = Standalone::new();
    daw.seed_project(daw::service::ProjectInfo {
        guid: "slip".into(),
        name: "Slip".into(),
        path: String::new(),
    });
    daw
}

fn mics(daw: &Standalone, names: &[&str]) -> Vec<ItemRef> {
    let ctx = ProjectContext::Current;
    names
        .iter()
        .map(|name| {
            let track = Tracks::add(daw, ctx.clone(), name, None).unwrap();
            Midi::create_midi_item(daw, ctx.clone(), TrackRef::Guid(track), 0.0, TAKE_LEN)
                .expect("item")
                .item
        })
        .collect()
}

fn pieces_on(daw: &Standalone, item: &ItemRef) -> Vec<(f64, f64, f64)> {
    let ctx = ProjectContext::Current;
    let info = daw.get_item(ctx.clone(), item.clone()).expect("item");
    let mut out: Vec<(f64, f64, f64)> = daw
        .get_items(ctx.clone(), TrackRef::Guid(info.track_guid.clone()))
        .into_iter()
        .map(|i| {
            let offset = daw
                .get_take(ctx.clone(), ItemRef::Guid(i.guid.clone()), TakeRef::Active)
                .map(|t| t.start_offset.as_seconds())
                .unwrap_or(0.0);
            (i.position.as_seconds(), i.length.as_seconds(), offset)
        })
        .collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    out
}

// r[verify drums.verify.slip-gesture]
#[test]
fn one_slip_cuts_and_slides_every_mic_identically() {
    let daw = project();
    let items = mics(&daw, &["Kick In", "Kick Out", "Kick Sub"]);
    let cfg = SplitConfig {
        leading_pad_secs: 0.005,
        crossfade_secs: 0.005,
    };

    // Drag the hit at 1.0 s 30 ms later; the next hit is at 1.5 s.
    let applied = slip_hit(
        &daw,
        ProjectContext::Current,
        &items,
        1.0,
        1.5,
        TAKE_LEN,
        0.030,
        cfg,
    )
    .expect("slip applied");
    assert_eq!(applied.items, 3);
    assert_eq!(applied.pieces, 9, "three pieces per mic");

    let first = pieces_on(&daw, &items[0]);
    for item in &items[1..] {
        assert_eq!(pieces_on(&daw, item), first, "mics cut identically");
    }

    // Only the dragged span moved: the middle piece plays 30 ms later
    // than its source position, the outer two are where they were cut.
    assert_eq!(first.len(), 3);
    let (lead, mid, tail) = (first[0], first[1], first[2]);
    assert!((lead.0 - 0.0).abs() < 1e-9 && lead.2 == 0.0);
    assert!(
        (mid.0 - (0.995 + 0.030)).abs() < 1e-9,
        "middle placed at cut+delta, got {}",
        mid.0
    );
    assert!((mid.2 - 0.995).abs() < 1e-9, "middle reads from its cut");
    assert!((tail.0 - 1.495).abs() < 1e-9, "tail did not move");
}

// r[verify drums.verify.slip-gesture]
// r[verify drums.quantize.undo]
#[test]
fn one_undo_step_restores_every_mic() {
    use daw::service::Projects;
    let daw = project();
    let items = mics(&daw, &["Kick In", "Kick Out", "Kick Sub"]);
    let ctx = ProjectContext::Current;
    let before: Vec<_> = items.iter().map(|i| pieces_on(&daw, i)).collect();

    daw.begin_undo_block(ctx.clone(), "Slip hit");
    slip_hit(
        &daw,
        ctx.clone(),
        &items,
        1.0,
        1.5,
        TAKE_LEN,
        0.030,
        SplitConfig {
            leading_pad_secs: 0.005,
            crossfade_secs: 0.005,
        },
    )
    .expect("slip applied");
    daw.end_undo_block(ctx.clone(), "Slip hit", None);

    assert_eq!(
        daw.last_undo_label(ctx.clone()).as_deref(),
        Some("Slip hit")
    );
    assert!(daw.undo(ctx.clone()), "one undo step");
    for (i, item) in items.iter().enumerate() {
        assert_eq!(pieces_on(&daw, item), before[i], "mic {i} restored");
    }
    assert!(!daw.undo(ctx.clone()), "and only one");

    // Redo brings the whole group edit back.
    assert!(daw.redo(ctx.clone()));
    let first = pieces_on(&daw, &items[0]);
    assert_eq!(first.len(), 3, "redo reinstated the split");
}
