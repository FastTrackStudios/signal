//! Every scene, as data, against its committed row list.
//!
//! The `.rows` half of a scene fixture (spec #48's amendment): which
//! rows survived the folds, how deep each sits and how wide it opens,
//! compared **byte for byte**. This is the half that carries the
//! decision, it is the same on every machine, and it is what the scene
//! engine is actually responsible for — the `.png` beside it is the
//! renderer's half and is gated structurally by
//! `apps/session-daw/tests/golden_scenes.rs`.
//!
//! Resolved here from the golden session's own tree rather than through
//! the bench, so the loop is milliseconds and a wrong rule is a diff
//! rather than a render.

use dynamic_template::golden_session::{fixtures_dir, maximal, rpp::flatten, vocal_fx};
use dynamic_template::scenes::{self, Size, Surface};

/// The panel height every committed row list was written at.
const PANEL: f64 = 1440.0;

/// One scene's row list, in the bench's own format: depth, name, width.
fn rows_of(slug: &str) -> String {
    let scene = scenes::scene(slug).unwrap_or_else(|| panic!("no scene {slug}"));
    let shape = if slug.starts_with("lead-vocal") {
        vocal_fx()
    } else {
        maximal()
    };
    let flats = flatten(&shape);
    let facts = scenes::from_flat(&flats);
    let mut text = String::new();
    for row in scenes::resolve(scene, &facts, Surface::Mixer, None, None, None) {
        let name = match &row.target {
            // A performer header carries no guid, only a name — the
            // same name the window's synthetic header track is given
            // (`session_daw::plan::apply_scene`), so this text agrees
            // with what the bench actually renders
            // (`apps/session-daw/tests/golden_scenes.rs`).
            scenes::Target::PerformerHeader(performer) => performer.clone(),
            scenes::Target::Track(guid) => flats
                .iter()
                .find(|f| &f.guid == guid)
                .map_or(String::new(), |f| f.name.clone()),
        };
        let width = scenes::TABLES.width(row.size, PANEL).max(1.0).round();
        text.push_str(&format!("{}\t{name}\t{width}\n", row.depth));
    }
    // The widths are whole pixels in the committed file.
    text.replace(".0\n", "\n")
}

fn committed(slug: &str) -> String {
    let path = fixtures_dir().join("scenes").join(format!("{slug}.rows"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The acceptance criterion: all nine scenes, as data, reproduce the row
/// list their hand-written functions produced.
///
/// r[verify flow.scenes.render]
/// r[verify flow.drums.tracking.full]
/// r[verify flow.drums.mixing.scenes]
/// r[verify flow.vocals.mixing.main]
/// r[verify flow.vocals.mixing.fx]
#[test]
fn every_scene_resolves_to_its_committed_row_list() {
    // `FTS_UPDATE_GOLDEN=1` rewrites instead of failing, the same way
    // the picture fixtures do. Folded into the comparison rather than
    // kept as a test of its own: two tests in one binary, one writing
    // what the other reads, race each other and pass by luck.
    let updating = std::env::var_os("FTS_UPDATE_GOLDEN").is_some();
    let mut failures = Vec::new();
    for scene in scenes::scenes() {
        let fresh = rows_of(&scene.slug);
        if updating {
            let path = fixtures_dir()
                .join("scenes")
                .join(format!("{}.rows", scene.slug));
            std::fs::write(&path, &fresh).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            continue;
        }
        let want = committed(&scene.slug);
        if fresh != want {
            let (n, a, b) = fresh
                .lines()
                .zip(want.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map_or((0, "", ""), |(n, (a, b))| (n + 1, a, b));
            failures.push(format!(
                "{}: {} rows, committed {}; first difference at row {n}:\n    \
                 resolved:  {a}\n    committed: {b}",
                scene.slug,
                fresh.lines().count(),
                want.lines().count()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A scene that resolves to nothing visible fails its fixture.
///
/// The negative control for the test above: a committed row list of
/// nothing could be written once and never looked at again, so an empty
/// resolution has to be an error on its own terms rather than a fixture
/// that happens to match.
#[test]
fn a_scene_resolving_to_no_rows_fails_its_fixture() {
    for scene in scenes::scenes() {
        assert!(
            rows_of(&scene.slug).lines().count() > 0,
            "{} resolves to nothing",
            scene.slug
        );
    }
    // And the control: a scene whose default hides everything shows
    // none of the session, and this test would catch it.
    //
    // Not *nothing*, though — the common prelude is not one of the
    // scene's rules and does not go away with them, so the Guide and
    // Keyflow folders survive, collapsed, exactly as
    // `flow.scenes.guide-folder` says they must in every scene. That
    // the two are independent is worth asserting rather than assuming.
    let mut blind = scenes::scene("drum-mixing").expect("a scene").clone();
    blind.default = scenes::Effect::hidden();
    blind.rules.clear();
    let facts = scenes::from_flat(&flatten(&maximal()));
    let rows = scenes::resolve(&blind, &facts, Surface::Mixer, None, None, None);
    let survived: Vec<&str> = rows
        .iter()
        .filter_map(|row| row.guid())
        .filter_map(|guid| facts.iter().find(|f| f.guid == guid))
        .map(|fact| fact.name.as_str())
        .collect();
    assert!(
        survived
            .iter()
            .all(|name| *name == "Guide" || *name == "Keyflow"),
        "a scene that hides everything still showed the session: {survived:?}"
    );
    assert!(
        !survived.contains(&"Kick"),
        "the blind scene showed an instrument"
    );
}

/// The two surfaces are two tables: the same scene gives a strip a width
/// and a row a height, and neither number is the other.
///
/// Matched by guid rather than by position. The two row lists used to be
/// the same length and this test zipped them, which stopped being true
/// the moment a scene used `Rule::arrange` to drop the parallel colour
/// out of the panel — the surfaces are allowed to disagree about WHICH
/// rows there are, and what is being asserted here is only that a row
/// present on both gets each table's own number.
#[test]
fn a_scene_sizes_both_surfaces_from_its_own_table() {
    let flats = flatten(&maximal());
    let facts = scenes::from_flat(&flats);
    let scene = scenes::scene("drum-mixing").expect("a scene");
    let mixer = scenes::resolve(scene, &facts, Surface::Mixer, None, None, None);
    let arrange = scenes::resolve(scene, &facts, Surface::Arrange, None, None, None);
    let worked = mixer
        .iter()
        .find(|row| row.size == Size::Working)
        .expect("something is worked on");
    let guid = worked.guid().expect("a worked row is a track");
    let same = arrange
        .iter()
        .find(|row| row.guid() == Some(guid))
        .expect("the panel dropped a track the mixer works on");
    assert!((scenes::TABLES.width(worked.size, PANEL) - 133.0).abs() < f64::EPSILON);
    // The mixer opens the piece to working width; the arrangement keeps
    // it a row like any other instrument's — the drums are not taller.
    assert_ne!(
        same.size,
        Size::Working,
        "the panel opened a piece to working height"
    );
    let other = arrange
        .iter()
        .find(|row| {
            row.guid().is_some_and(|g| {
                flats
                    .iter()
                    .any(|f| f.guid == g && f.path.ends_with("Electric/Rhythm"))
            })
        })
        .expect("the electric's Rhythm is in the panel");
    assert_eq!(
        scenes::TABLES.height(same.size),
        scenes::TABLES.height(other.size),
        "a drum piece is a different height from the rest of the band"
    );
}

/// The panel may show FEWER rows than the mixer, never more.
///
/// A scene can take a row out of the arrangement and leave the strip —
/// that is what `Rule::arrange` is for, and Drum Mixing uses it for the
/// parallel colour. The other direction has no meaning: a track you can
/// edit and cannot mix is not a state any flow wants, and a rule that
/// produced one would be a mistake rather than a choice.
///
#[test]
fn the_panel_never_shows_a_row_the_mixer_hides() {
    let facts = scenes::from_flat(&flatten(&maximal()));
    for scene in scenes::scenes() {
        let of = |surface| -> std::collections::HashSet<String> {
            scenes::resolve(scene, &facts, surface, None, None, None)
                .iter()
                .filter_map(|row| row.guid())
                .map(str::to_owned)
                .collect()
        };
        let extra: Vec<String> = of(Surface::Arrange)
            .difference(&of(Surface::Mixer))
            .cloned()
            .collect();
        assert!(
            extra.is_empty(),
            "{} shows rows in the panel that its mixer hides: {extra:?}",
            scene.slug
        );
    }
}

/// **Drum Tracking Overview**: the kit as the drummer reads it. One row
/// per piece, collapsed and big enough to carry an arm and a monitor
/// lamp, and **no mic rows at all** — at this zoom a mic is a line, and
/// thirty lines under a kit is the picture the scene exists to remove.
///
/// r[verify flow.drums.tracking.overview]
/// r[verify flow.drums.tracking.arm]
#[test]
fn the_tracking_overview_is_one_row_per_piece_and_no_mics() {
    let rows = rows_of("drum-tracking-overview");
    let named: Vec<&str> = rows
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .collect();

    for piece in ["Kick", "Snare", "Toms", "Cymbals", "Rooms"] {
        assert!(
            named.contains(&piece),
            "the drummer cannot see {piece}: {named:?}"
        );
    }
    for mic in ["In", "Out", "Top", "Bottom", "Trig"] {
        assert!(
            !named.contains(&mic),
            "a mic row ({mic}) reached the overview: {named:?}"
        );
    }
}

/// **Drum Editing**: each piece, and under it where its recording is —
/// its mics and its trigger, lifted out of the Sum that adds them up —
/// and nothing made from them: no sub, sample or reverb return, no Sum
/// row, no Process folder or bus tree, because editing is about takes
/// and a send has no hits in it. The pieces start shut in the window,
/// which folds them; the scene leaves them openable.
///
/// r[verify flow.drums.editing.scene]
#[test]
fn drum_editing_shows_where_the_recording_is() {
    let rows = rows_of("drum-editing");
    let named: Vec<&str> = rows
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .collect();

    for piece in ["Kick", "Snare", "Toms"] {
        assert!(named.contains(&piece), "no {piece} row to edit: {named:?}");
    }
    // The kit's rows: from Drums to what follows it.
    let kit: Vec<&str> = named
        .iter()
        .skip_while(|n| **n != "Drums")
        .take_while(|n| **n != "Percussion")
        .copied()
        .collect();
    for recorded in ["In", "Out", "Top", "Trig"] {
        assert!(
            kit.contains(&recorded),
            "no {recorded} under its piece: {kit:?}"
        );
    }
    for gone in ["Sum", "Sub", "Sample", "Fund", "Verb"] {
        assert!(
            !kit.contains(&gone),
            "the kit's {gone} reached the edit scene: {kit:?}"
        );
    }
    for gone in ["Process", "MIX BUS"] {
        assert!(
            !named.contains(&gone),
            "{gone} reached the edit scene: {named:?}"
        );
    }
    // The mics sit right under their piece, the Sum lifted away.
    let depth = |name: &str| {
        rows.lines()
            .find(|l| l.split('\t').nth(1) == Some(name))
            .and_then(|l| l.split('\t').next()?.trim().parse::<u32>().ok())
    };
    assert_eq!(
        depth("In"),
        depth("Kick").map(|d| d + 1),
        "In a level under Kick"
    );
}

/// **Guitar Balance**: the one guitar scene that opens the multi-mic
/// level, which is exactly what Guitar Mixing hides.
///
/// Almost all of a guitar mix happens at the part and its bus, so
/// Mixing collapses every layer and multi-mic folder. Setting a
/// configuration's initial balance and panning needs the opposite, and
/// that is what this scene is for — so the test that matters is that
/// the two disagree about the sources.
///
/// r[verify flow.guitars.mixing.balance-scene]
#[test]
fn guitar_balance_opens_what_guitar_mixing_hides() {
    let sources = |slug: &str| -> usize {
        rows_of(slug)
            .lines()
            .filter(|line| {
                // A source row is one the mixing scene keeps at its
                // minimum and the balance scene opens: count the rows
                // deep enough to be under a channel.
                line.split('\t')
                    .next()
                    .and_then(|depth| depth.parse::<u32>().ok())
                    .is_some_and(|depth| depth >= 3)
            })
            .count()
    };
    let balance = sources("guitar-balance");
    let mixing = sources("guitar-mixing");
    assert!(
        balance > mixing,
        "balance shows {balance} deep rows and mixing {mixing} — \
         the balance scene is supposed to open what mixing hides"
    );
}

/// **Vocal Tracking** keeps the FX returns in view, which looks like a
/// mixing concern and is not: a singer needs to hear the reverb they
/// are singing into, and a tracking view that hid the returns would
/// leave the engineer unable to set what the performer hears.
///
/// r[verify flow.vocals.tracking]
#[test]
fn vocal_tracking_keeps_the_returns_the_singer_hears() {
    let rows = rows_of("vocal-tracking");
    let named: Vec<&str> = rows
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .collect();
    assert!(
        named.iter().any(|n| *n == "Ron"),
        "no lead to track: {named:?}"
    );
    assert!(
        named
            .iter()
            .any(|n| n.contains("Verb") || n.contains("Delay")),
        "the returns the singer hears are missing: {named:?}"
    );
    assert!(
        !named.contains(&"MIX BUS"),
        "the bus tree reached a tracking view"
    );
}

/// **Vocal Comping** opens the parts and their sources — a part comps
/// on its folder the way a kit does, so a many-layer "Hey!" is comped
/// once rather than once per layer.
///
/// r[verify flow.vocals.comping]
#[test]
fn vocal_comping_opens_the_parts_and_their_layers() {
    let rows = rows_of("vocal-comping");
    let named: Vec<&str> = rows
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .collect();
    assert!(named.iter().any(|n| *n == "BGVs"), "{named:?}");
    assert!(
        named.iter().any(|n| *n == "Hey"),
        "the many-layer part is missing: {named:?}"
    );
    assert!(
        named.iter().filter(|n| n.starts_with("All ")).count() > 1,
        "its layers are not open to comp: {named:?}"
    );
}
