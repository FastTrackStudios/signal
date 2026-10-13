//! Growing a guitar part — headless, no REAPER, no window.
//!
//! r[verify flow.guitars.dimensions]
//! r[verify flow.guitars.grow]
//! r[verify flow.guitars.mixing.source-defaults]
//! r[verify flow.guitars.acoustics]
//!
//! These are scenario tests in the sense #48's testing decisions mean it:
//! the real gesture goes through the facade (`session::guitar_grow::
//! GuitarGrow` over `daw_standalone::sync::Standalone`, the same trait
//! impl production wires to `daw::reaper::Reaper`) and the project is
//! read back through the same facade. Nothing here drives the session
//! window, and nothing asserts on how the grower got there — only on the
//! tree, the names the classifier reads back out of it, the items, the
//! sends, the pans and the mutes.
//!
//! Every scenario carries its negative control: the source a user moved
//! is untouched, the items never move, the sends never change.
//!
//! Run: cargo test -p session --test `guitar_grow_actions`

use daw::service::{Items, ProjectContext, Routing, TrackRef, Tracks, TracksExt};
use daw_proto::{ProjectInfo, Projects};
use daw_standalone::sync::Standalone;
use dynamic_template::track_schema::{TrackDimension, classify_track_dimension};
use session::guitar_grow::{GuitarGrow, GuitarGrowActions};

// ── harness ─────────────────────────────────────────────────────────

fn setup() -> (Standalone, GuitarGrow<Standalone>) {
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "guitar-grow".into(),
        name: "Guitar Grow".into(),
        path: String::new(),
    });
    let grow = GuitarGrow::new(daw.clone());
    (daw, grow)
}

/// Run one action the way the REAPER `ActionBackend` runs it: bracketed
/// in a host undo block, so the whole gesture is one undo step. The
/// `#[action(undo)]` flag is what asks for this in production; here the
/// test supplies the bracket so `one_undo` can assert on it.
fn as_one_action<T>(daw: &Standalone, label: &str, body: impl FnOnce() -> T) -> T {
    daw.begin_undo_block(ProjectContext::Current, label);
    let out = body();
    daw.end_undo_block(ProjectContext::Current, label, None);
    out
}

/// The whole project as `(name, folder_depth)` in mixer order — the flat
/// encoding the DAW itself stores, so an assertion on it is an assertion
/// on the real tree.
fn flat(daw: &Standalone) -> Vec<(String, i32)> {
    let mut all = daw.all(ProjectContext::Current);
    all.sort_by_key(|t| t.index);
    all.into_iter().map(|t| (t.name, t.folder_depth)).collect()
}

/// Every track as the dimension path the classifier reads it back into:
/// the names on the way down from the root, each with the dimension the
/// template says that name is. This is the naming round-trip the whole
/// ticket turns on — name → classify → the same path.
fn dimension_paths(daw: &Standalone) -> Vec<(String, TrackDimension)> {
    let tree = daw.track_tree();
    let mut all = daw.all(ProjectContext::Current);
    all.sort_by_key(|t| t.index);
    all.into_iter()
        .map(|track| {
            // The outermost ancestor's name, not the parent's: the
            // classifier reads a name *with* its context, so "L DI" comes
            // back as a Channel. The name that says which instrument's
            // vocabulary applies is the one at the top of the chain.
            let mut root = track.clone();
            while let Some(parent) = tree.parent_of(&root) {
                root = parent.clone();
            }
            let dimension = classify_track_dimension(&track.name, &[root.name]);
            (path_of(daw, &track.guid), dimension)
        })
        .collect()
}

fn path_of(daw: &Standalone, guid: &str) -> String {
    let tree = daw.track_tree();
    let mut names = Vec::new();
    let mut current = tree.get(guid).cloned();
    while let Some(track) = current {
        names.push(track.name.clone());
        current = tree.parent_of(&track).cloned();
    }
    names.reverse();
    names.join("/")
}

/// Every item in the project as `(track name, item guid)` — the check
/// that a gesture left items where they were.
fn items_by_track(daw: &Standalone) -> Vec<(String, String)> {
    let mut all = daw.all(ProjectContext::Current);
    all.sort_by_key(|t| t.index);
    let mut out = Vec::new();
    for track in all {
        for item in daw.get_items(ProjectContext::Current, TrackRef::Guid(track.guid.clone())) {
            out.push((track.name.clone(), item.guid));
        }
    }
    out.sort();
    out
}

/// Every send in the project as `(source guid, destination guid)`.
///
/// Guids, not names or indices: a gesture inserts tracks and may hand a
/// container's arrangement name down to the folder that now holds it, and
/// neither of those is a change to the routing. What must not change is
/// which track feeds which.
fn sends_by_track(daw: &Standalone) -> Vec<(String, String)> {
    let mut all = daw.all(ProjectContext::Current);
    all.sort_by_key(|t| t.index);
    let mut out = Vec::new();
    for track in all {
        for route in daw.sends(ProjectContext::Current, TrackRef::Guid(track.guid.clone())) {
            out.push((
                route.source_track_guid,
                route.dest_track_guid.unwrap_or_default(),
            ));
        }
    }
    out.sort();
    out
}

fn track_named(daw: &Standalone, path: &str) -> daw_proto::Track {
    let mut all = daw.all(ProjectContext::Current);
    all.sort_by_key(|t| t.index);
    all.into_iter()
        .find(|t| path_of(daw, &t.guid) == path)
        .unwrap_or_else(|| panic!("no track at {path}; tree is {:#?}", flat(daw)))
}

fn select(daw: &Standalone, path: &str) {
    let track = track_named(daw, path);
    TracksExt::select(daw, &track.guid).unwrap();
}

/// A part that has never been grown: one track carrying one item, which
/// is what `flow.guitars.dimensions` says a single-source, single-channel
/// part looks like — no folder anywhere, because no level has a second
/// member yet.
fn one_di_track(daw: &Standalone, name: &str) -> String {
    let guid = daw.insert_track(name).unwrap();
    daw.add_item(
        ProjectContext::Current,
        TrackRef::Guid(guid.clone()),
        daw_proto::PositionInSeconds::from_seconds(0.0),
        daw_proto::Duration::from_seconds(4.0),
    )
    .expect("the part's take");
    // A bus the part feeds, so "routing untouched" has something to be
    // untouched about.
    let bus = daw.insert_track("ELECTRIC BUS").unwrap();
    daw.add_send(
        ProjectContext::Current,
        TrackRef::Guid(guid.clone()),
        TrackRef::Guid(bus),
    );
    guid
}

fn pan_of(daw: &Standalone, path: &str) -> f64 {
    track_named(daw, path).pan
}

fn muted(daw: &Standalone, path: &str) -> bool {
    track_named(daw, path).muted
}

// ── the scenario ────────────────────────────────────────────────────

/// From one DI track, all four gestures in turn, on an electric part.
///
/// The assertions at each step are the ticket's contract: the folder
/// level appears only as the level gains its second member, the new
/// names classify back into the dimension they were created as, and the
/// item and the send never move.
#[test]
fn one_di_track_grows_through_all_four_actions() {
    let (daw, grow) = setup();
    one_di_track(&daw, "GTR E Rhythm");
    let items_before = items_by_track(&daw);
    let sends_before = sends_by_track(&daw);

    // ── double: the Channel level gains its second member, so and only
    // so does the folder appear. The item goes with the content it was
    // on, onto L.
    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "double", || grow.double()).expect("double");
    assert_eq!(
        flat(&daw),
        vec![
            ("GTR E Rhythm".to_string(), 1),
            ("L".to_string(), 0),
            ("R".to_string(), -1),
            ("ELECTRIC BUS".to_string(), 0),
        ]
    );

    // ── add a source, three times: DI is already what the part carries,
    // so the next sources are the pedalboard and the two amps, and each
    // lands on *every* channel.
    for step in 0..3 {
        select(&daw, "GTR E Rhythm");
        as_one_action(&daw, "add source", || grow.add_source())
            .unwrap_or_else(|e| panic!("add_source {step}: {e}"));
    }
    assert_eq!(
        flat(&daw),
        vec![
            ("GTR E Rhythm".to_string(), 1),
            ("L".to_string(), 1),
            ("DI".to_string(), 0),
            ("Pedalboard".to_string(), 0),
            ("Amp 1".to_string(), 1),
            ("SM57".to_string(), 0),
            ("Royer".to_string(), -1),
            ("Amp 2".to_string(), 1),
            ("SM57".to_string(), 0),
            ("Royer".to_string(), -2),
            ("R".to_string(), 1),
            ("DI".to_string(), 0),
            ("Pedalboard".to_string(), 0),
            ("Amp 1".to_string(), 1),
            ("SM57".to_string(), 0),
            ("Royer".to_string(), -1),
            ("Amp 2".to_string(), 1),
            ("SM57".to_string(), 0),
            ("Royer".to_string(), -3),
            ("ELECTRIC BUS".to_string(), 0),
        ]
    );

    // ── add a layer: the Layer level appears, the channels fold under
    // Main, and the next voice the template offers mirrors it whole.
    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "add layer", || grow.add_layer()).expect("add_layer");
    let names: Vec<String> = flat(&daw).into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&"Main".to_string()));
    assert!(names.contains(&"OCT".to_string()));

    // ── add an arrangement: the part track was carrying "Rhythm" in its
    // own name, so that word moves down onto the folder that now holds it
    // and the next arrangement the template offers arrives beside it. The
    // template's own order decides which that is — for an electric it
    // lists Clean first — the gesture does not pick a favourite.
    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "add arrangement", || grow.add_arrangement()).expect("add_arrangement");
    let names: Vec<String> = flat(&daw).into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&"Rhythm".to_string()), "tree is {names:#?}");
    assert!(names.contains(&"Clean".to_string()), "tree is {names:#?}");
    assert!(
        names.contains(&"GTR E".to_string()),
        "the container drops the arrangement it handed down; tree is {names:#?}"
    );

    // ── the round trip: every name the actions wrote classifies back
    // into the dimension it was written as.
    let paths = dimension_paths(&daw);
    let expected: Vec<(&str, TrackDimension)> = vec![
        ("GTR E/Rhythm", TrackDimension::Arrangement),
        ("GTR E/Rhythm/Main", TrackDimension::Layer),
        ("GTR E/Rhythm/Main/L", TrackDimension::Channel),
        ("GTR E/Rhythm/Main/L/DI", TrackDimension::MultiMic),
        ("GTR E/Rhythm/Main/L/Pedalboard", TrackDimension::MultiMic),
        ("GTR E/Rhythm/Main/L/Amp 1", TrackDimension::MultiMic),
        ("GTR E/Rhythm/Main/L/Amp 1/SM57", TrackDimension::MultiMic),
        ("GTR E/Rhythm/Main/L/Amp 1/Royer", TrackDimension::MultiMic),
        ("GTR E/Rhythm/Main/R", TrackDimension::Channel),
        ("GTR E/Rhythm/OCT", TrackDimension::Layer),
        ("GTR E/Clean", TrackDimension::Arrangement),
        ("GTR E/Clean/Main", TrackDimension::Layer),
        ("GTR E/Clean/Main/L", TrackDimension::Channel),
    ];
    for (path, dimension) in expected {
        let found = paths
            .iter()
            .find(|(p, _)| p == path)
            .unwrap_or_else(|| panic!("no track at {path}; paths are {paths:#?}"));
        assert_eq!(
            found.1, dimension,
            "{path} should classify as {dimension}, got {}",
            found.1
        );
    }

    // ── the negative controls. Still the one item, still the same item,
    // and on the track that took over carrying the part's content — the
    // DI of the L channel of the Main layer of the Rhythm arrangement.
    // That last assertion is the one that matters: "leaves items and
    // routing where they were" is about the *performance* staying put as
    // folders grow above it, and the take ends up on the one leaf that
    // is still a track rather than a folder. Folding what was there into
    // L, and then into L's DI, is exactly what these gestures are for —
    // a folder never carries the take its children sum.
    let items_after = items_by_track(&daw);
    assert_eq!(items_before.len(), 1);
    assert_eq!(items_after.len(), 1, "an item was created or lost");
    assert_eq!(
        items_before[0].1, items_after[0].1,
        "the item is a different item"
    );
    let take_holder = track_named(&daw, "GTR E/Rhythm/Main/L/DI");
    assert_eq!(
        daw.get_items(ProjectContext::Current, TrackRef::Guid(take_holder.guid))
            .len(),
        1,
        "the part's take is not on the L channel's DI; items are {items_after:#?}"
    );
    assert_eq!(
        sends_before,
        sends_by_track(&daw),
        "a send was added, removed or repointed"
    );
}

/// The boundary of having no flag, asserted rather than left to be
/// discovered.
///
/// `flow.guitars.mixing.source-defaults` says "the default applies to
/// what has never been set", and without a flag the only reading of
/// "never been set" is centred and unmuted. So an engineer who un-mutes
/// a DI and leaves it centred reads as untouched, and the next source
/// added to that channel mutes it again. Moving it anywhere at all makes
/// the choice legible — which is the case
/// `a_user_moved_source_is_untouched_by_a_further_add_source` covers.
#[test]
fn an_unmuted_but_still_centred_di_reads_as_never_set() {
    let (daw, grow) = setup();
    one_di_track(&daw, "GTR E Rhythm");

    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "double", || grow.double()).expect("double");
    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "add source", || grow.add_source()).expect("add_source");
    assert!(muted(&daw, "GTR E Rhythm/L/DI"));

    let di = track_named(&daw, "GTR E Rhythm/L/DI");
    Tracks::set_muted(
        &daw,
        ProjectContext::Current,
        TrackRef::Guid(di.guid),
        false,
    )
    .unwrap();

    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "add source", || grow.add_source()).expect("add_source");

    assert!(
        muted(&daw, "GTR E Rhythm/L/DI"),
        "a centred, unmuted DI is indistinguishable from one nobody has \
         balanced, so the next source mutes it again — pan it to keep it"
    );
}

/// Each gesture is one undo step: the whole of it reverts, and nothing
/// but the whole of it.
#[test]
fn each_action_reverts_in_one_undo() {
    let (daw, grow) = setup();
    one_di_track(&daw, "GTR E Rhythm");

    let mut gestures: Vec<(&str, Box<dyn Fn() -> daw::service::DawResult<()> + '_>)> = Vec::new();
    gestures.push(("double", Box::new(|| grow.double())));
    gestures.push(("add source", Box::new(|| grow.add_source())));
    gestures.push(("add layer", Box::new(|| grow.add_layer())));
    gestures.push(("add arrangement", Box::new(|| grow.add_arrangement())));

    for (label, gesture) in gestures {
        let before = flat(&daw);
        // Whatever the previous gesture renamed, act on the part again.
        let part = daw
            .all(ProjectContext::Current)
            .into_iter()
            .min_by_key(|t| t.index)
            .unwrap();
        TracksExt::select(&daw, &part.guid).unwrap();

        as_one_action(&daw, label, &gesture).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_ne!(flat(&daw), before, "{label} changed nothing");

        assert!(
            daw.undo(ProjectContext::Current),
            "{label} left no undo step"
        );
        assert_eq!(
            flat(&daw),
            before,
            "{label} did not revert in a single undo"
        );

        // Put it back so the next gesture has the grown part to work on.
        assert!(daw.redo(ProjectContext::Current));
    }
}

/// A two-amp channel reads back the balance the rule gives it: the DI
/// muted and centred, the pedalboard muted beside the amps, each amp's
/// 57 and 121 hard left and right, and each amp's own track 60 % to its
/// side.
#[test]
fn a_created_two_amp_configuration_reads_back_its_pans_and_mutes() {
    let (daw, grow) = setup();
    one_di_track(&daw, "GTR E Rhythm");

    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "double", || grow.double()).expect("double");
    for _ in 0..3 {
        select(&daw, "GTR E Rhythm");
        as_one_action(&daw, "add source", || grow.add_source()).expect("add_source");
    }

    for side in ["L", "R"] {
        let at = |leaf: &str| format!("GTR E Rhythm/{side}/{leaf}");

        // The DI is the reamp and the safety, not the sound.
        assert!(muted(&daw, &at("DI")), "{side} DI should be muted");
        assert!((pan_of(&daw, &at("DI"))).abs() < f64::EPSILON);

        // The amp takes priority over the pedalboard.
        assert!(
            muted(&daw, &at("Pedalboard")),
            "{side} Pedalboard should be muted beside an amp"
        );

        // Both mics of both amps are heard, each amp leaning to its side.
        assert!((pan_of(&daw, &at("Amp 1")) + 0.6).abs() < 1e-9);
        assert!((pan_of(&daw, &at("Amp 2")) - 0.6).abs() < 1e-9);
        for amp in ["Amp 1", "Amp 2"] {
            assert!((pan_of(&daw, &at(&format!("{amp}/SM57"))) + 1.0).abs() < 1e-9);
            assert!((pan_of(&daw, &at(&format!("{amp}/Royer"))) - 1.0).abs() < 1e-9);
            assert!(!muted(&daw, &at(&format!("{amp}/SM57"))));
        }
    }
}

/// The negative control on the defaults: a source an engineer has moved
/// keeps its place when the next source arrives. There is no flag saying
/// so — a track that has been panned is simply not one of the tracks the
/// default applies to.
#[test]
fn a_user_moved_source_is_untouched_by_a_further_add_source() {
    let (daw, grow) = setup();
    one_di_track(&daw, "GTR E Rhythm");

    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "double", || grow.double()).expect("double");
    for _ in 0..2 {
        select(&daw, "GTR E Rhythm");
        as_one_action(&daw, "add source", || grow.add_source()).expect("add_source");
    }

    // The engineer moves L's first amp somewhere of their own, and
    // un-mutes the DI they want to hear.
    let amp = track_named(&daw, "GTR E Rhythm/L/Amp 1");
    Tracks::set_pan(
        &daw,
        ProjectContext::Current,
        TrackRef::Guid(amp.guid),
        0.25,
    )
    .unwrap();
    let di = track_named(&daw, "GTR E Rhythm/L/DI");
    Tracks::set_muted(
        &daw,
        ProjectContext::Current,
        TrackRef::Guid(di.guid.clone()),
        false,
    )
    .unwrap();
    Tracks::set_pan(&daw, ProjectContext::Current, TrackRef::Guid(di.guid), -0.4).unwrap();

    // A second amp arrives. On R — untouched — the rule applies in full.
    select(&daw, "GTR E Rhythm");
    as_one_action(&daw, "add source", || grow.add_source()).expect("add_source");

    assert!(
        (pan_of(&daw, "GTR E Rhythm/L/Amp 1") - 0.25).abs() < 1e-9,
        "the moved amp was re-defaulted"
    );
    assert!(
        (pan_of(&daw, "GTR E Rhythm/L/DI") + 0.4).abs() < 1e-9,
        "the moved DI was re-defaulted"
    );
    assert!(
        !muted(&daw, "GTR E Rhythm/L/DI"),
        "the un-muted DI was re-muted"
    );

    assert!((pan_of(&daw, "GTR E Rhythm/R/Amp 1") + 0.6).abs() < 1e-9);
    assert!((pan_of(&daw, "GTR E Rhythm/R/Amp 2") - 0.6).abs() < 1e-9);
    assert!(muted(&daw, "GTR E Rhythm/R/DI"));
}

/// The same four gestures on an acoustic part, with the acoustic's own
/// vocabulary and no code of their own: Arrangement (Strum), Layer
/// (Main, OCT), Channel (L, R), MultiMic (DI, Neck, Body).
#[test]
fn an_acoustic_part_grows_by_the_same_rules() {
    let (daw, grow) = setup();
    one_di_track(&daw, "GTR A Strum");
    let items_before = items_by_track(&daw);
    let sends_before = sends_by_track(&daw);

    select(&daw, "GTR A Strum");
    as_one_action(&daw, "double", || grow.double()).expect("double");
    for _ in 0..2 {
        select(&daw, "GTR A Strum");
        as_one_action(&daw, "add source", || grow.add_source()).expect("add_source");
    }
    select(&daw, "GTR A Strum");
    as_one_action(&daw, "add layer", || grow.add_layer()).expect("add_layer");
    select(&daw, "GTR A Strum");
    as_one_action(&daw, "add arrangement", || grow.add_arrangement()).expect("add_arrangement");

    let paths = dimension_paths(&daw);
    let expected: Vec<(&str, TrackDimension)> = vec![
        ("GTR A/Strum", TrackDimension::Arrangement),
        ("GTR A/Strum/Main", TrackDimension::Layer),
        ("GTR A/Strum/Main/L", TrackDimension::Channel),
        ("GTR A/Strum/Main/L/DI", TrackDimension::MultiMic),
        ("GTR A/Strum/Main/L/Neck", TrackDimension::MultiMic),
        ("GTR A/Strum/Main/L/Body", TrackDimension::MultiMic),
        ("GTR A/Strum/Main/R", TrackDimension::Channel),
        ("GTR A/Strum/OCT", TrackDimension::Layer),
        ("GTR A/Down", TrackDimension::Arrangement),
    ];
    for (path, dimension) in expected {
        let found = paths
            .iter()
            .find(|(p, _)| p == path)
            .unwrap_or_else(|| panic!("no track at {path}; paths are {paths:#?}"));
        assert_eq!(found.1, dimension, "{path} classifies as {}", found.1);
    }

    // The DI beside two mics is muted and centred; neither condenser is
    // a 57 or a 121, so both stay where the engineer will put them.
    assert!(muted(&daw, "GTR A/Strum/Main/L/DI"));
    assert!((pan_of(&daw, "GTR A/Strum/Main/L/DI")).abs() < f64::EPSILON);
    assert!(!muted(&daw, "GTR A/Strum/Main/L/Neck"));
    assert!((pan_of(&daw, "GTR A/Strum/Main/L/Neck")).abs() < f64::EPSILON);
    assert!((pan_of(&daw, "GTR A/Strum/Main/L/Body")).abs() < f64::EPSILON);

    assert_eq!(items_before.len(), 1);
    assert_eq!(items_by_track(&daw).len(), 1);
    assert_eq!(sends_before, sends_by_track(&daw));
}

/// The negative control on the folder rule: a level with one member is
/// never a folder. A part nobody has doubled is one track, and the
/// grower does not spend a folder making that true.
#[test]
fn a_level_with_one_member_is_not_a_folder() {
    let (daw, _grow) = setup();
    one_di_track(&daw, "GTR E Lead");

    assert_eq!(
        flat(&daw),
        vec![
            ("GTR E Lead".to_string(), 0),
            ("ELECTRIC BUS".to_string(), 0),
        ]
    );
}

/// Nothing selected is an error the user sees, not a silent no-op.
#[test]
fn growing_with_no_selection_is_an_error() {
    let (daw, grow) = setup();
    one_di_track(&daw, "GTR E Rhythm");
    Tracks::set_selected(&daw, ProjectContext::Current, TrackRef::Index(0), false).unwrap();

    assert!(grow.double().is_err());
    assert!(grow.add_layer().is_err());
    assert!(grow.add_arrangement().is_err());
    assert!(grow.add_source().is_err());
}
