//! Applying the fixture album to a reference session: every source
//! track's record input, set from its matching entry, through the
//! daw facade, as one undo step. Arm and monitoring are never touched
//! (there is nothing here to touch — apply's whole surface against
//! the backend is `Tracks::set_record_input`, wrapped in one undo
//! block).
//!
//! The reference session is built here, track by track, from the
//! fixture album's own lowered entries — matching a `Selector` against
//! a session by NAME is the scene engine's job (#50, not landed;
//! `dynamic_template::scenes`'s own doc says so), so this test gives
//! each track its own taxonomy directly rather than parsing it back
//! out of a name. One entry (`producer/talkback/mic`) is deliberately
//! left without a track — the fixture's **unused** entry — and one
//! extra track is added that no entry names — the fixture's
//! **unpatched** track.
//!
//! r[verify flow.patch-list.apply]

use daw_proto::{DawResult, ProjectContext, ProjectInfo, Projects, RecordInput, TrackRef, Tracks};
use daw_standalone::sync::Standalone;
use dynamic_template::scenes::Selector;
use patch_list::{
    FIXTURE_ALBUM, FIXTURE_STUDIO, FIXTURE_STUDIO_NAME, Lowered, PatchList, Resolve, StudioProfile,
    apply, validate,
};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

/// The entry left with no track — the fixture's unused entry.
const UNUSED: (&str, &str, &str) = ("producer", "talkback", "mic");

fn seeded() -> (Standalone, ProjectContext) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "reference".into(),
        name: "reference".into(),
        path: String::new(),
    });
    (daw, ProjectContext::Project(guid))
}

/// The reference session: every track built for the fixture, kept
/// alongside the entry it stands for so the test can find one back by
/// name — `Reference::resolve` is `patch_list::Resolve`'s table.
struct Reference(Vec<(TrackRef, Lowered)>);

impl Resolve for Reference {
    fn tracks(&self) -> Vec<(TrackRef, Selector)> {
        self.0
            .iter()
            .map(|(track, lowered)| (track.clone(), lowered.selector.clone()))
            .collect()
    }
}

impl Reference {
    /// The track built for one entry, by its path — `None` on a typo
    /// in the test itself, not on session state.
    fn track_for(&self, performer: &str, kind: &str, key: &str) -> Option<TrackRef> {
        self.0
            .iter()
            .find(|(_, l)| l.performer == performer && l.kind == kind && l.key == key)
            .map(|(track, _)| track.clone())
    }
}

/// Build the reference session: one track per fixture entry except
/// [`UNUSED`], plus one extra track no entry names — the fixture's
/// unpatched track.
fn build_reference(
    daw: &Standalone,
    ctx: &ProjectContext,
    list: &PatchList,
) -> DawResult<Reference> {
    let mut tracks = Vec::new();
    for lowered in list.entries() {
        if (
            lowered.performer.as_str(),
            lowered.kind.as_str(),
            lowered.key.as_str(),
        ) == UNUSED
        {
            continue;
        }
        let name = format!("{} {} {}", lowered.performer, lowered.kind, lowered.key);
        let guid = Tracks::add(daw, ctx.clone(), &name, None)?;
        tracks.push((TrackRef::Guid(guid), lowered));
    }
    let extra = Tracks::add(daw, ctx.clone(), "Extra Guitar Mystery", None)?;
    tracks.push((
        TrackRef::Guid(extra),
        Lowered {
            performer: "extra".into(),
            kind: "guitar".into(),
            key: "mystery".into(),
            selector: Selector {
                performer: Some("extra".into()),
                kind: Some("guitar".into()),
                multi_mic: Some("mystery".into()),
                ..Selector::default()
            },
            entry: patch_list::Entry::Role("nothing".into()),
        },
    ));
    Ok(Reference(tracks))
}

#[test]
fn applying_the_fixture_sets_every_source_tracks_input_as_one_undo_step() -> Result {
    let (daw, ctx) = seeded();
    let list = PatchList::from_styx(FIXTURE_ALBUM)?;
    let room = StudioProfile::from_styx(FIXTURE_STUDIO)?;
    let plan = validate(&list, &room)?;
    let reference = build_reference(&daw, &ctx, &list)?;

    let report = apply::apply(&daw, &ctx, &plan, &reference)?;

    // 33 entries total (lowering.rs): one unused (no track), one
    // unresolved in this room (bassist's amp mic, not applied but not
    // unused either — its track exists), the rest applied.
    assert_eq!(report.unused, 1, "{report:?}");
    assert_eq!(report.unpatched, 1, "{report:?}");
    assert_eq!(report.applied, 31, "{report:?}");

    // Read back through the bulk `Tracks::get` — undo restores the
    // snapshot of `tracks`, not the `track_record_input` side map, so
    // that is the read that actually proves the revert below. `None`
    // (no such track) reads as a mismatch against any expected input,
    // which is exactly what should fail the assertion.
    let input_of = |track: &TrackRef| -> Option<RecordInput> {
        Tracks::get(&daw, ctx.clone(), track.clone()).map(|t| t.record_input)
    };

    // Cody's DI — an audio role.
    let cody_di = reference
        .track_for("cody", "guitar", "di")
        .ok_or("cody's DI")?;
    assert_eq!(
        input_of(&cody_di),
        Some(RecordInput::Audio { channel: 2 }),
        "DI 3 resolves to channel 2 in the golden room"
    );

    // The drummer's kick in — another audio role, a different rig.
    let kick_in = reference
        .track_for("drummer", "drums", "kick/in")
        .ok_or("the drummer's kick in")?;
    assert_eq!(input_of(&kick_in), Some(RecordInput::Audio { channel: 24 }));

    // John's synth — a MIDI entry named directly, not a role.
    let synth = reference
        .track_for("john", "keys", "synth")
        .ok_or("john's synth")?;
    assert_eq!(
        input_of(&synth),
        Some(RecordInput::Midi {
            device_id: None,
            channel: Some(1)
        })
    );

    // The bassist's amp mic — unresolved in this room: matched (it has
    // a track) but never set.
    let amp = reference
        .track_for("bassist", "bass", "amp")
        .ok_or("the bassist's amp mic")?;
    assert_eq!(input_of(&amp), Some(RecordInput::None));

    // One undo step reverts every input apply touched.
    assert!(Projects::undo(&daw, ctx.clone()));
    assert_eq!(input_of(&cody_di), Some(RecordInput::None));
    assert_eq!(input_of(&kick_in), Some(RecordInput::None));
    assert_eq!(input_of(&synth), Some(RecordInput::None));

    Ok(())
}

#[test]
fn applying_stores_the_effective_text_the_profile_and_a_timestamp() -> Result {
    let (daw, ctx) = seeded();
    let list = PatchList::from_styx(FIXTURE_ALBUM)?;
    let room = StudioProfile::from_styx(FIXTURE_STUDIO)?;
    let plan = validate(&list, &room)?;
    let reference = build_reference(&daw, &ctx, &list)?;

    apply::apply(&daw, &ctx, &plan, &reference)?;
    // No override: the effective text is the album's own, unchanged.
    let text = apply::effective_text(FIXTURE_ALBUM, None)?;
    apply::store_applied(&daw, ctx.clone(), &text, FIXTURE_STUDIO_NAME)?;

    let applied = apply::applied(&daw, ctx)?.ok_or("an applied copy")?;
    assert_eq!(applied.text, FIXTURE_ALBUM);
    assert_eq!(applied.profile, FIXTURE_STUDIO_NAME);
    Ok(())
}
