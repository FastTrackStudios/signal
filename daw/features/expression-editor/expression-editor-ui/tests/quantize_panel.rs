//! The quantize surface (#165).
//!
//! The engine was complete and unreachable. These cover the surface's
//! own claims: that the plan is previewable before anything is written,
//! that "why did that hit not move" is answerable, and that it works for
//! MIDI notes as well as audio transients.

use expression_editor_tools::event::Timed;
use expression_editor_tools::quantize::QuantizeConfig;
use expression_editor_ui::quantize_panel::{
    Group, QuantizePanel, WriteMode, excluded_count, histogram, preview, threshold_position,
};

/// A hit with a strength — stands in for an audio transient.
#[derive(Clone, Copy, Debug)]
struct Hit {
    at: f64,
    weight: f64,
}

impl Timed for Hit {
    fn onset(&self) -> f64 {
        self.at
    }
    fn move_to(&mut self, to: f64) {
        self.at = to;
    }
    fn strength(&self) -> f64 {
        self.weight
    }
}

/// A MIDI note — the other half of "this is a tool panel, not an audio
/// panel".
#[derive(Clone, Copy, Debug)]
struct Note {
    start: f64,
    velocity: f64,
}

impl Timed for Note {
    fn onset(&self) -> f64 {
        self.start
    }
    fn move_to(&mut self, to: f64) {
        self.start = to;
    }
    fn strength(&self) -> f64 {
        self.velocity
    }
}

fn panel(grid: f64) -> QuantizePanel {
    QuantizePanel {
        config: QuantizeConfig {
            grid,
            ..QuantizeConfig::default()
        },
        ..Default::default()
    }
}

fn late_hits() -> Vec<Hit> {
    (1..5)
        .map(|i| Hit {
            at: i as f64 + 0.02,
            weight: 0.9,
        })
        .collect()
}

#[test]
fn a_plan_is_previewable_before_anything_is_written() {
    let (plan, view) = preview(&late_hits(), &panel(1.0), 400.0, (0.0, 5.0));
    assert_eq!(plan.moves.len(), 4);
    assert_eq!(view.lines.len(), 4);
    assert!(
        view.lines.iter().all(|l| l.moves()),
        "every line shows a move"
    );
}

#[test]
fn the_grid_is_drawn_so_the_target_is_visible_not_implied() {
    let (_, view) = preview(&late_hits(), &panel(1.0), 500.0, (0.0, 5.0));
    assert_eq!(view.divisions.len(), 6, "0 through 5 inclusive");
    assert!(view.divisions.iter().all(|x| (0.0..=500.0).contains(x)));
}

#[test]
fn an_unmatched_hit_is_shown_rather_than_omitted() {
    // `Plan::unmatched` exists so "why did that hit not move" is
    // answerable, and a line that is simply absent answers nothing.
    let hits = vec![
        Hit {
            at: 1.0,
            weight: 0.9,
        },
        Hit {
            at: 1.5,
            weight: 0.05,
        },
    ];
    let mut p = panel(1.0);
    p.config.min_strength = 0.5;

    let (plan, view) = preview(&hits, &p, 400.0, (0.0, 3.0));
    assert!(!plan.unmatched.is_empty(), "the weak hit was left alone");
    let weak: Vec<_> = view.lines.iter().filter(|l| l.unmatched).collect();
    assert_eq!(weak.len(), 1, "and it is on screen");
    assert!(!weak[0].moves(), "drawn where it is, not where it would go");
}

#[test]
fn lines_are_ordered_left_to_right() {
    let hits = vec![
        Hit {
            at: 3.02,
            weight: 0.9,
        },
        Hit {
            at: 1.02,
            weight: 0.9,
        },
    ];
    let (_, view) = preview(&hits, &panel(1.0), 400.0, (0.0, 4.0));
    for w in view.lines.windows(2) {
        assert!(
            w[0].x <= w[1].x,
            "a preview that jumps around is unreadable"
        );
    }
}

#[test]
fn the_panel_works_for_midi_notes_too() {
    // Quantize is a tool, so this is a tool panel — not an audio panel.
    let notes: Vec<Note> = (1..4)
        .map(|i| Note {
            start: i as f64 + 0.03,
            velocity: 0.8,
        })
        .collect();
    let (plan, view) = preview(&notes, &panel(1.0), 300.0, (0.0, 4.0));
    assert_eq!(plan.moves.len(), 3);
    assert_eq!(view.lines.len(), 3);
}

#[test]
fn geometry_stays_inside_the_panel() {
    let hits = vec![
        Hit {
            at: -5.0,
            weight: 0.9,
        },
        Hit {
            at: 500.0,
            weight: 0.9,
        },
    ];
    let (_, view) = preview(&hits, &panel(1.0), 200.0, (0.0, 5.0));
    for l in &view.lines {
        assert!((0.0..=200.0).contains(&l.x), "x escaped: {}", l.x);
        assert!((0.0..=200.0).contains(&l.to_x));
    }
}

// ── The sensitivity histogram ────────────────────────────────────────

#[test]
fn the_histogram_shows_where_the_hits_actually_sit() {
    let hits = vec![
        Hit {
            at: 0.0,
            weight: 0.05,
        },
        Hit {
            at: 1.0,
            weight: 0.09,
        },
        Hit {
            at: 2.0,
            weight: 0.90,
        },
        Hit {
            at: 3.0,
            weight: 0.95,
        },
    ];
    let bins = histogram(&hits, 10);
    assert_eq!(bins.len(), 10);
    assert_eq!(bins[0].count, 2, "the quiet cluster");
    assert_eq!(bins[9].count, 2, "and the loud one");
    assert_eq!(bins.iter().map(|b| b.count).sum::<usize>(), 4);
}

#[test]
fn the_top_of_the_range_lands_in_the_last_bin_not_off_the_end() {
    let hits = vec![Hit {
        at: 0.0,
        weight: 1.0,
    }];
    let bins = histogram(&hits, 4);
    assert_eq!(bins[3].count, 1);
}

#[test]
fn the_threshold_is_placed_on_the_same_scale_as_the_bins() {
    // What makes the slider legible against the material's own floor.
    let mut cfg = QuantizeConfig {
        min_strength: 0.3,
        ..Default::default()
    };
    assert_eq!(threshold_position(&cfg), 0.3);
    cfg.min_strength = 5.0;
    assert_eq!(threshold_position(&cfg), 1.0, "clamped onto the axis");
}

#[test]
fn the_panel_can_say_how_much_it_is_excluding() {
    let hits = vec![
        Hit {
            at: 0.0,
            weight: 0.1,
        },
        Hit {
            at: 1.0,
            weight: 0.2,
        },
        Hit {
            at: 2.0,
            weight: 0.9,
        },
    ];
    let cfg = QuantizeConfig {
        min_strength: 0.5,
        ..Default::default()
    };
    assert_eq!(excluded_count(&hits, &cfg), 2);
}

// ── Mode and group ───────────────────────────────────────────────────

#[test]
fn split_is_the_default_mode() {
    // Drum editing is done by splitting, because it is phase-coherent
    // across a mic group.
    assert_eq!(QuantizePanel::default().mode, WriteMode::Split);
}

#[test]
fn a_group_needs_its_trigger_to_be_one_of_its_members() {
    // Otherwise the group is edited against a reference nobody can hear.
    let mut g = Group {
        members: vec!["snare".into(), "oh-l".into()],
        trigger: Some("snare".into()),
    };
    assert!(g.is_valid());

    g.trigger = Some("a-track-not-in-the-group".into());
    assert!(!g.is_valid());

    g.trigger = None;
    assert!(!g.is_valid(), "something has to be detected from");
}

// ── Grid options ─────────────────────────────────────────────────────

// r[verify drums.quantize.grid-options]
#[test]
fn the_grid_offers_straight_triplet_and_dotted_from_quarter_to_sixty_fourth() {
    use expression_editor_ui::quantize_panel::{GridDivision, GridFeel};

    assert_eq!(GridDivision::ALL.len(), 5, "1/4 through 1/64");
    assert_eq!(GridFeel::ALL.len(), 3);

    let mut p = QuantizePanel::default();
    // One beat = 0.5 s (120 bpm). A straight 1/16 is a quarter of it.
    p.division = GridDivision::Sixteenth;
    p.feel = GridFeel::Straight;
    assert!((p.grid_in(0.5) - 0.125).abs() < 1e-12);
    // A triplet 1/16 is two thirds of that; a dotted one, half again.
    p.feel = GridFeel::Triplet;
    assert!((p.grid_in(0.5) - 0.125 * 2.0 / 3.0).abs() < 1e-12);
    p.feel = GridFeel::Dotted;
    assert!((p.grid_in(0.5) - 0.1875).abs() < 1e-12);
}

// r[verify drums.quantize.grid-options]
#[test]
fn swing_delays_only_the_off_beat_targets() {
    // Hits near the odd divisions (1 and 3 — the off-beats of a pair)
    // and one near an even division (2 — on the beat).
    let hits = vec![
        Hit {
            at: 1.01,
            weight: 0.9,
        },
        Hit {
            at: 2.01,
            weight: 0.9,
        },
        Hit {
            at: 3.01,
            weight: 0.9,
        },
    ];
    let mut p = panel(1.0);
    p.swing = 1.0; // full triplet feel
    let (plan, _) = preview(&hits, &p, 400.0, (0.0, 4.0));
    assert_eq!(plan.moves.len(), 3);
    let to: Vec<f64> = plan.moves.iter().map(|m| m.to).collect();
    assert!(
        (to[0] - (1.0 + 1.0 / 3.0)).abs() < 1e-9,
        "odd division moved to the triplet position: {to:?}"
    );
    assert!((to[1] - 2.0).abs() < 1e-9, "on-beat is untouched: {to:?}");
    assert!(
        (to[2] - (3.0 + 1.0 / 3.0)).abs() < 1e-9,
        "odd division moved to the triplet position: {to:?}"
    );
}

#[test]
fn grid_scan_toggle_is_the_tolerance_switch() {
    let mut p = QuantizePanel::default();
    p.grid_scan = true;
    p.tolerance = 0.03;
    p.sync_config();
    assert_eq!(p.config.tolerance, Some(0.03));

    p.grid_scan = false;
    p.sync_config();
    assert_eq!(p.config.tolerance, None, "every hit to its own nearest");

    p.grid_scan = true;
    p.sync_config();
    assert_eq!(
        p.config.tolerance,
        Some(0.03),
        "toggling did not forget the dialled value"
    );
}

// ── Filter presets ───────────────────────────────────────────────────

// r[verify drums.quantize.filter-presets]
#[test]
fn the_shipped_presets_put_each_drum_where_it_lives() {
    use expression_editor_ui::quantize_panel::FilterPreset;

    let names: Vec<String> = FilterPreset::builtins()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, vec!["Full kit", "Kick", "Snare", "Toms"]);

    let mut p = QuantizePanel::default();
    let kick = p.presets.iter().position(|f| f.name == "Kick").unwrap();
    p.apply_preset(kick);
    assert_eq!(p.detect.high_pass_hz, Some(30.0));
    assert_eq!(p.detect.low_pass_hz, Some(800.0));

    // Full kit is flat — a preset, not an absence of one.
    p.apply_preset(0);
    assert_eq!(p.detect.high_pass_hz, None);
    assert_eq!(p.detect.low_pass_hz, None);
}

// r[verify drums.quantize.filter-presets]
#[test]
fn a_user_preset_captures_the_dialled_filters_and_is_selected() {
    let mut p = QuantizePanel::default();
    p.detect.high_pass_hz = Some(90.0);
    p.detect.low_pass_hz = Some(3_000.0);
    p.save_preset("My snare");
    assert_eq!(p.presets.last().unwrap().name, "My snare");
    assert_eq!(p.preset, p.presets.len() - 1);

    // Round-trips: dial something else, come back.
    p.apply_preset(0);
    assert_eq!(p.detect.high_pass_hz, None);
    let mine = p.presets.iter().position(|f| f.name == "My snare").unwrap();
    p.apply_preset(mine);
    assert_eq!(p.detect.high_pass_hz, Some(90.0));
    assert_eq!(p.detect.low_pass_hz, Some(3_000.0));
}

// ── Slider defaults ──────────────────────────────────────────────────

// r[verify drums.quantize.slider-defaults]
#[test]
fn right_click_stores_and_alt_click_resets() {
    use expression_editor_ui::quantize_panel::SliderDefaults;

    let mut d = SliderDefaults::default();
    // Nothing stored: Alt-click falls back to the built-in default.
    assert_eq!(d.reset_value("sensitivity", 0.5), 0.5);

    d.store("sensitivity", 0.72);
    assert_eq!(d.get("sensitivity"), Some(0.72));
    assert_eq!(d.reset_value("sensitivity", 0.5), 0.72);

    // Storing again overwrites rather than accumulating.
    d.store("sensitivity", 0.31);
    assert_eq!(d.reset_value("sensitivity", 0.5), 0.31);
    // Other sliders are untouched.
    assert_eq!(d.reset_value("strength", 1.0), 1.0);
}

// ── Preview data ─────────────────────────────────────────────────────

// r[verify drums.quantize.preview]
#[test]
fn hit_previews_count_moved_and_excluded() {
    use expression_editor_ui::quantize_panel::hit_previews;

    let hits = vec![
        Hit {
            at: 1.02, // late — moves
            weight: 0.9,
        },
        Hit {
            at: 2.0, // already on its division — matched but unmoved
            weight: 0.9,
        },
        Hit {
            at: 2.5, // between divisions, outside tolerance — excluded
            weight: 0.9,
        },
    ];
    let mut p = panel(1.0);
    p.config.tolerance = Some(0.1);
    let (plan, _) = preview(&hits, &p, 400.0, (0.0, 4.0));
    let previews = hit_previews(&plan);

    assert_eq!(previews.len(), 3, "every hit is in the list");
    assert_eq!(previews.iter().filter(|h| h.moved).count(), 1);
    assert_eq!(previews.iter().filter(|h| h.excluded).count(), 1);
    let excluded = previews.iter().find(|h| h.excluded).unwrap();
    assert_eq!(excluded.at, excluded.to, "dimmed where it is, not moved");
    // Sorted by current position, ready to draw left to right.
    for w in previews.windows(2) {
        assert!(w[0].at <= w[1].at);
    }
}

// ── The panel on the real surface ────────────────────────────────────

mod surface {
    use std::cell::RefCell;

    use dioxus::prelude::*;
    use dioxus_test::{by_testid, render};
    use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
    use expression_editor_core::{Editor, Mode, Tool, Viewport};
    use expression_editor_ui::ExpressionEditor;

    const PPQ: f64 = 960.0;

    thread_local! {
        static STAGED: RefCell<Option<Editor>> = const { RefCell::new(None) };
    }

    fn stage(ed: Editor) {
        STAGED.with(|s| *s.borrow_mut() = Some(ed));
    }

    fn editor_in(mode: Mode) -> Editor {
        let space = mode.default_row_space();
        let (lo, hi) = space.bounds();
        let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
        let span = (hi - lo).max(1);
        for i in 0..4u64 {
            let row = lo + (span * (i as i32 + 1)) / 6;
            let start = PPQ * i as f64 * 0.9;
            doc.push(Note::new(NoteId(i + 1), start, start + PPQ * 0.7, row));
        }
        doc.row_space = space;
        let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
        ed.set_mode(mode);
        ed.tool = Tool::Select;
        ed.reset_view();
        ed
    }

    #[component]
    fn Surface() -> Element {
        let editor = use_signal(|| STAGED.with(|s| s.borrow_mut().take()).expect("staged"));
        rsx! {
            ExpressionEditor { editor }
        }
    }

    // r[verify drums.quantize.panel]
    #[tokio::test]
    async fn the_quantize_tool_is_on_the_toolbar_in_unpitched_audio() -> dioxus_test::Result<()> {
        stage(editor_in(Mode::UnpitchedAudio));
        let tester = render(Surface).with_window_size(1000, 620).build();
        tester
            .query(by_testid("tool-quantize"))
            .immediately()
            .expect("the mode that edits hits offers the quantizer");
        Ok(())
    }

    // r[verify drums.quantize.panel]
    #[tokio::test]
    async fn the_tool_is_absent_where_there_is_nothing_to_detect() -> dioxus_test::Result<()> {
        stage(editor_in(Mode::Midi));
        let tester = render(Surface).with_window_size(1000, 620).build();
        assert!(
            tester
                .query(by_testid("tool-quantize"))
                .immediately()
                .is_err(),
            "MIDI mode quantizes through the grid, not the drawer"
        );
        Ok(())
    }

    // r[verify drums.quantize.panel]
    #[tokio::test]
    async fn clicking_the_tool_opens_the_drawer_with_its_sections() -> dioxus_test::Result<()> {
        stage(editor_in(Mode::UnpitchedAudio));
        let tester = render(Surface).with_window_size(1000, 620).build();
        assert!(
            tester
                .query(by_testid("quantize-panel"))
                .immediately()
                .is_err(),
            "closed until asked for"
        );

        tester
            .query(by_testid("tool-quantize"))
            .immediately()?
            .click();
        tester.drain();
        tester.relayout();

        tester.query(by_testid("quantize-panel")).immediately()?;
        // The sections' own controls, top to bottom: detect, target,
        // write, apply.
        tester
            .query(by_testid("quantize-histogram"))
            .immediately()?;
        tester
            .query(by_testid("qslider-sensitivity"))
            .immediately()?;
        tester.query(by_testid("quantize-preset")).immediately()?;
        tester
            .query(by_testid("quantize-grid-scan"))
            .immediately()?;
        tester
            .query(by_testid("quantize-mode-split"))
            .immediately()?;
        tester.query(by_testid("quantize-apply")).immediately()?;

        // SPLIT is the default, so pad and crossfade are offered.
        tester.query(by_testid("qslider-pad")).immediately()?;

        // And the toggle closes it again.
        tester
            .query(by_testid("tool-quantize"))
            .immediately()?
            .click();
        tester.drain();
        tester.relayout();
        assert!(
            tester
                .query(by_testid("quantize-panel"))
                .immediately()
                .is_err(),
            "the same button closes the drawer"
        );
        Ok(())
    }

    // r[verify drums.quantize.panel]
    #[tokio::test]
    async fn the_advanced_disclosure_reveals_the_per_kit_controls() -> dioxus_test::Result<()> {
        stage(editor_in(Mode::UnpitchedAudio));
        let tester = render(Surface).with_window_size(1000, 620).build();
        tester
            .query(by_testid("tool-quantize"))
            .immediately()?
            .click();
        tester.drain();
        tester.relayout();

        assert!(
            tester
                .query(by_testid("qslider-retrigger"))
                .immediately()
                .is_err(),
            "advanced is folded by default"
        );
        tester
            .query(by_testid("quantize-advanced"))
            .immediately()?
            .click();
        tester.drain();
        tester.relayout();
        tester.query(by_testid("qslider-crest")).immediately()?;
        tester.query(by_testid("qslider-low-cut")).immediately()?;
        tester.query(by_testid("qslider-high-cut")).immediately()?;
        tester.query(by_testid("qslider-retrigger")).immediately()?;
        tester.query(by_testid("qslider-offset")).immediately()?;
        Ok(())
    }
}
