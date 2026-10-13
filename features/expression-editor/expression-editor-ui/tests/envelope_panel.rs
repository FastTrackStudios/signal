//! The vocal envelope surface.
//!
//! Everything here is about *what am I editing* and *what can I see* —
//! the two questions four overlaid crossing curves make hard.

use expression_editor_ui::envelopes::{ActiveEnvelope, EnvelopePanel, Shown, traces};
use level_dsp::envelope::{Bypass, Contributions, EnvPoint, GainSpan};

fn parts() -> Contributions {
    Contributions {
        gate: vec![
            EnvPoint {
                t_s: 0.0,
                db: -60.0,
                hold: true,
            },
            EnvPoint {
                t_s: 0.5,
                db: 0.0,
                hold: true,
            },
        ],
        breath: vec![EnvPoint::new(0.0, 0.0), EnvPoint::new(1.0, -12.0)],
        ride: vec![EnvPoint::new(0.0, 3.0), EnvPoint::new(1.0, -3.0)],
        sibilance: vec![GainSpan {
            from_s: 0.2,
            to_s: 0.3,
            db: -4.0,
        }],
        bypass: Bypass::default(),
    }
}

#[test]
fn the_ride_is_active_by_default() {
    let panel = EnvelopePanel::default();
    assert_eq!(panel.active, ActiveEnvelope::Ride);
    assert_eq!(panel.drag_target(), ActiveEnvelope::Ride);
}

#[test]
fn a_drag_goes_to_the_active_envelope_whatever_is_nearer() {
    // The trap this avoids: with four curves crossing, hit-testing by
    // proximity silently edits the gate when you meant the ride.
    let mut panel = EnvelopePanel::default();
    assert_eq!(panel.drag_target(), ActiveEnvelope::Ride);
    panel.activate(ActiveEnvelope::Gate);
    assert_eq!(
        panel.drag_target(),
        ActiveEnvelope::Gate,
        "the answer depends on the highlight, not the cursor"
    );
}

#[test]
fn exactly_one_is_active_and_it_is_visibly_distinct() {
    let panel = EnvelopePanel::default();
    let t = traces(&parts(), &panel, 100.0, 50.0, 1.0, 24.0);
    let active: Vec<_> = t.iter().filter(|x| x.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].which, Some(ActiveEnvelope::Ride));
}

#[test]
fn overlays_toggle_independently() {
    let mut shown = Shown::default();
    assert_eq!(shown.shown_count(), 4);
    shown.toggle(ActiveEnvelope::Gate);
    assert!(!shown.is_shown(ActiveEnvelope::Gate));
    assert!(
        shown.is_shown(ActiveEnvelope::Ride),
        "the others are untouched"
    );
    assert_eq!(shown.shown_count(), 3);
}

#[test]
fn hiding_an_overlay_removes_it_from_the_drawing_only() {
    let mut panel = EnvelopePanel::default();
    let p = parts();
    let before = traces(&p, &panel, 100.0, 50.0, 1.0, 24.0).len();

    panel.shown.toggle(ActiveEnvelope::Gate);
    let after = traces(&p, &panel, 100.0, 50.0, 1.0, 24.0);

    assert_eq!(after.len(), before - 1);
    // Hiding is not bypassing: the composite still includes the gate.
    let comp = after.iter().find(|t| t.is_composite).unwrap();
    assert!(
        comp.points.iter().any(|(_, y)| *y > 40.0),
        "the gate is still audible in the composite even though hidden"
    );
}

#[test]
fn solo_shows_one_and_hides_the_rest() {
    let mut shown = Shown::default();
    shown.solo(ActiveEnvelope::Breath);
    assert_eq!(shown.shown_count(), 1);
    assert!(shown.is_shown(ActiveEnvelope::Breath));
}

#[test]
fn activating_something_hidden_shows_it() {
    // Editing a curve you cannot see is worse than an extra click.
    let mut panel = EnvelopePanel::default();
    panel.shown.toggle(ActiveEnvelope::Gate);
    assert!(!panel.shown.is_shown(ActiveEnvelope::Gate));

    panel.activate(ActiveEnvelope::Gate);
    assert!(panel.shown.is_shown(ActiveEnvelope::Gate));
}

#[test]
fn cycling_reaches_every_envelope_and_wraps() {
    let mut panel = EnvelopePanel::default();
    let mut seen = vec![panel.active];
    for _ in 0..3 {
        panel.cycle();
        seen.push(panel.active);
    }
    for e in ActiveEnvelope::ALL {
        assert!(seen.contains(&e), "{e:?} was unreachable");
    }
    panel.cycle();
    assert_eq!(panel.active, ActiveEnvelope::Ride, "and wraps");
}

#[test]
fn the_composite_draws_alongside_the_four() {
    let t = traces(&parts(), &EnvelopePanel::default(), 100.0, 50.0, 1.0, 24.0);
    assert_eq!(t.iter().filter(|x| x.is_composite).count(), 1);
    assert_eq!(t.iter().filter(|x| !x.is_composite).count(), 4);
}

#[test]
fn the_composite_still_draws_when_every_overlay_is_hidden() {
    // It is what the DAW plays; a panel showing nothing at all would be
    // indistinguishable from a broken one.
    let mut panel = EnvelopePanel::default();
    for e in ActiveEnvelope::ALL {
        panel.shown.toggle(e);
    }
    let t = traces(&parts(), &panel, 100.0, 50.0, 1.0, 24.0);
    assert_eq!(t.len(), 1);
    assert!(t[0].is_composite);
}

#[test]
fn hiding_all_but_one_leaves_a_single_curve_editing_surface() {
    let mut panel = EnvelopePanel::default();
    panel.shown.solo(ActiveEnvelope::Ride);
    panel.shown.composite = false;
    let t = traces(&parts(), &panel, 100.0, 50.0, 1.0, 24.0);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].which, Some(ActiveEnvelope::Ride));
    assert!(t[0].active);
}

#[test]
fn geometry_stays_inside_the_panel() {
    let t = traces(&parts(), &EnvelopePanel::default(), 200.0, 80.0, 1.0, 24.0);
    for trace in &t {
        for (x, y) in &trace.points {
            assert!((0.0..=200.0).contains(x), "x out of panel: {x}");
            assert!((0.0..=80.0).contains(y), "y out of panel: {y}");
        }
    }
}

#[test]
fn a_sibilance_span_draws_as_a_rectangle_not_a_ramp() {
    let panel = EnvelopePanel::default();
    let t = traces(&parts(), &panel, 100.0, 50.0, 1.0, 24.0);
    let sib = t
        .iter()
        .find(|x| x.which == Some(ActiveEnvelope::Sibilance))
        .unwrap();
    // Four corners per span: down, across, up.
    assert_eq!(sib.points.len(), 4);
    assert_eq!(sib.points[0].0, sib.points[1].0, "a vertical edge");
    assert_eq!(sib.points[2].0, sib.points[3].0, "and another");
}
