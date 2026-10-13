//! A comped item plays the take REAPER marked `SEL`, not an empty lane slot.
//!
//! Built from a real item in `set in stone` (Crescendum), where a tom
//! trigger track had been comped. REAPER writes an empty comp-lane slot as
//! `TAKE NULL` — it occupies a take *index*, exactly as REAPER counts takes
//! — and marks the active take `TAKE SEL`. Such an item has no inline first
//! take, so it carries no item-level `GUID`.
//!
//! Resolving the active take by that missing GUID found nothing and fell
//! back to index 0, which is a null slot. The item then reported an `Empty`
//! active take, and every consumer that keeps only audio dropped it: the
//! trigger tracks composed to 1.3% non-zero at -66 dBFS while their source
//! files were full 317-second recordings, which read as "the trigger tracks
//! hold no audio".

use daw_proto::{ItemRef, Items, ProjectContext, Takes, TrackRef, Tracks};
use daw_standalone::sync::Standalone;

/// One track, one comped item: three empty slots, a real take, another
/// empty slot, then the selected take. The shape of the real item.
const COMPED: &str = r#"<REAPER_PROJECT 0.1 "7.65" 1786573501
  SAMPLERATE 48000 0 0
  <TRACK {76C5CF39-EE77-234F-B94C-1A51752BA9B6}
    NAME "T1 Trig"
    <ITEM
      POSITION 0
      LENGTH 10
      IID 18864
      VOLPAN 1
      TAKE NULL
      TAKE NULL
      TAKE NULL
      TAKE
      NAME "early.wav"
      SOFFS 0
      GUID {73FC5666-87FC-C043-AC85-0A3FBE0880CE}
      <SOURCE WAVE
        FILE "early.wav"
      >
      TAKE NULL
      TAKE SEL
      NAME "keeper.wav"
      SOFFS 0
      GUID {779B1301-AF2E-0347-845D-B88EA21C2469}
      <SOURCE WAVE
        FILE "keeper.wav"
      >
    >
  >
>
"#;

fn only_item(daw: &Standalone, ctx: &ProjectContext) -> ItemRef {
    let track = Tracks::all(daw, ctx.clone())
        .into_iter()
        .find(|t| !t.is_folder)
        .expect("a track");
    let items = Items::get_items(daw, ctx.clone(), TrackRef::Guid(track.guid));
    ItemRef::Guid(items.first().expect("an item").guid.clone())
}

#[test]
fn the_selected_take_plays_not_the_first_null_slot() {
    let daw = Standalone::new();
    let loaded =
        daw_standalone::project_loader::load_rpp_text(&daw, "comped", "/tmp/comped.rpp", COMPED)
            .expect("load");
    let ctx = ProjectContext::Project(loaded.project_guid.clone());
    let item = only_item(&daw, &ctx);

    let active = Takes::get_active_take(&daw, ctx, item).expect("an active take");
    assert_eq!(
        active.name, "keeper.wav",
        "the TAKE SEL take must play; got {:?}",
        active.name
    );
}

#[test]
fn null_slots_still_occupy_take_indices() {
    // REAPER numbers empty comp lanes, so dropping them would silently
    // re-index every take on the item.
    let daw = Standalone::new();
    let loaded =
        daw_standalone::project_loader::load_rpp_text(&daw, "comped", "/tmp/comped.rpp", COMPED)
            .expect("load");
    let ctx = ProjectContext::Project(loaded.project_guid.clone());
    let item = only_item(&daw, &ctx);

    assert_eq!(
        Takes::take_count(&daw, ctx, item),
        6,
        "three nulls, a take, a null, then the selected take"
    );
}
