# Takes, fixed lanes and comp areas: what daw exposes today

Research for wayfinder issue #24. Every comping rule in
`docs/spec/session/workflows.md` (`flow.drums.comping.*`,
`flow.vocals.comping`, `flow.guitars.comping`, `flow.bass.comping`,
`flow.keys.comping`) needs three things from the daw facade: the takes of
an item, the fixed lanes of a track (which lane an item is on, which lanes
play, what each lane is called), and the comp areas that map a comp lane
back onto its source lanes. This note records what the sibling `../daw`
checkout provides for each, read and write, on both backends, against the
source as of 2026-09-14. Paths are relative to `../daw` unless they start
with `docs/` or `/home/`. The REAPER SDK is cited from the header the
`daw-reaper` crate actually links (the pinned `reaper-rs` checkout under
`~/.cargo`, referred to below as `SDK`:
`~/.cargo/git/checkouts/reaper-rs-35bd1a5aa8cde7cf/261645f/main/low/lib/reaper/reaper_plugin_functions.h`).

## Summary

1. **Takes are fully modelled and fully writable on both backends**: `Take`/`TakeRef` plus a 21-method `Takes` RPC (list, active take, add/delete, set active, name/colour/volume/rate/pitch/offset/source, take markers, take ratings), with `ActiveTakeChanged` on the event bus.
2. **Fixed lanes are modelled read-only in the proto** (`Track.lane_count/lane_play_mask/lane_names/lane_display`, `Item.fixed_lane`) and **only the standalone backend fills them** — from the RPP loader; the REAPER backend hard-codes zeros/`None` with "not yet wired" comments, though the SDK exposes `I_FIXEDLANE`, `I_NUMFIXEDLANES`, `C_LANEPLAYS:N`, `C_LANESETTINGS`, `P_LANENAME:n`.
3. **No service method writes a lane anywhere**: `Tracks` has no lane setters, `Items` has no `set_fixed_lane`; the only lane *writer* is the offline RPP builder (`fixed_lanes`/`lane_names`/`lane_record`/`fixed_lane`) used by the Pro Tools importer.
4. **Comp areas do not exist in daw at all**: REAPER writes them as `ITEMLANES n` + `LINKEDLANE start end comp_lane src_lane …` lines under `<TRACK>`; neither RPP loader parses them (unknown track tokens fall through `_ => {}`), the proto has no type for them, and `LANEREC`'s comping-lane fields are parsed but never surfaced.
5. **Standalone is behind REAPER on takes only where REAPER runs actions** (`run_take_rating_action` is a no-op) and **ahead on lanes** (it reads and honours the play mask in the renderer); the RPP re-import in `dawfile-standalone` decodes `FIXEDLANES` fields with a different meaning than `dawfile-reaper`, so the two loaders disagree on lane count and play mask.

## What daw-proto models

### Takes

- `TakeRef { Guid(String), Index(u32), Active }` — `crates/daw/proto/src/item/take.rs:12-19`.
- `Take` — guid, `item_guid`, `index`, `is_active`, name, colour, volume, `play_rate`, pitch, `preserve_pitch`, `channel_mode` (REAPER `CHANMODE`), `start_offset`, `source_type`, `source_file_path`, `source_length`, `source_sample_rate`, `source_channels`, `is_midi`, `midi_note_count` — `crates/daw/proto/src/item/take.rs:23-72`.
- `SourceType { Audio, Midi, Empty, Video, Unknown }` — `take.rs:77-89`.
- Take markers: `TakeMarker`, `TakeMarkerCreate`, `TakeMarkerUpdate`, `AddTakeMarkerAtPositionRequest` — `take.rs:141-189`. Positions are in source seconds, not project time (`take.rs:149-150`).
- Take ratings (REAPER 7.17 up-rank/down-rank, encoded as `:)`/`:))`/`:(` marker names): `TakeRating` with `from_marker_name`/`to_marker_name` — `take.rs:211-256`; the REAPER command IDs as constants in `take_rating_actions` — `take.rs:263-301`.
- A second, action-shaped surface: `TakeRankScope`, `TakeRankLevel`, the `TakeRankingService::apply_rank` RPC and the `TakeRankingActions` `#[architect::actions]` trait — `crates/daw/proto/src/take_ranking.rs:15-33, 39-50, 63-`.
- Events: `ItemEvent::ActiveTakeChanged { project_guid, item_guid, old_take_index, new_take_index }` — `crates/daw/proto/src/item/event.rs:62-67`; `TakeEvent::{Created, Deleted, NameChanged, PitchChanged, PlayRateChanged, VolumeChanged, SourceChanged}` — `event.rs:73-120`. Both ride the cross-domain bus as `DawEvent::Take` (`crates/daw/proto/src/event_bus/event.rs:43`).
- `Item.take_count` and `Item.active_take_index` on the item itself — `crates/daw/proto/src/item/item.rs:84-87`.
- The capability flag `Capability::Takes` ("multi-take support within items") — `crates/daw/proto/src/capability.rs:65-67`; it is part of the `TIMELINE_FULL` profile (`capability.rs:345-352`).

### Fixed lanes

- On the track: `lane_count: u32` (0 = lanes disabled), `lane_play_mask: u64` (bit n = lane n audible), `lane_names: Vec<String>` (may be shorter than `lane_count`), `lane_display: LaneDisplay` — `crates/daw/proto/src/track/track.rs:129-139`; `LaneDisplay { Small, Big, One }` mirrors REAPER's lane-button cycle — `track.rs:291-302`.
- On the item: `fixed_lane: Option<u32>` — "Fixed item lane this item sits on (REAPER 7 comping lanes). `None` when the track has no fixed lanes" — `crates/daw/proto/src/item/item.rs:79-81`.
- There is **no** lane event, no lane setter on `Tracks` (`crates/daw/proto/src/track/service.rs:31-200` has `set_muted … set_tcp_height`, nothing lane-shaped) and no `set_fixed_lane` on `Items` (`crates/daw/proto/src/item/service.rs:14-140`). The record/comp lane indices (`LANEREC`) are not represented.
- The "lane" methods that do exist on `Projects` are **ruler** lanes (marker/region lanes, REAPER 7.62), unrelated to item lanes: `ruler_lane_count`, `set_ruler_lane_name`, `get_ruler_lane_name` — `crates/daw/proto/src/project/service.rs:76` and `crates/daw/control/src/project.rs:468-577`. `Marker.lane`/`Region.lane` are ruler lanes too (`crates/daw/proto/src/marker/marker.rs:24`, `crates/daw/proto/src/region/region.rs:24`).

### Comp areas

Nothing. `grep -rn "comp_area\|CompArea\|LINKEDLANE\|ITEMLANES"` over `crates/daw` and the two dawfile crates returns no type, no field and no parser branch; the only hits for `LINKEDLANE` in the whole checkout are mouse-modifier context names in `features/reaper/reaper-input/src/input/mouse_modifiers/types.rs:272-278, 489-495`.

## What daw-control exposes

`daw-control` is the async handle layer over the `TakesClient`/`ItemsClient` (`crates/daw/control/src/lib.rs:128, 139, 234-235`).

| Handle | Method | R/W | Path |
|---|---|---|---|
| `ItemHandle` | `info() -> Item` (carries `take_count`, `active_take_index`, `fixed_lane`) | R | `crates/daw/control/src/items.rs:325` |
| `ItemHandle` | `takes() -> Takes`, `active_take() -> TakeHandle` | R | `items.rs:533-548` |
| `Takes` | `all() -> Vec<Take>`, `by_index(i)`, `active()` | R | `items.rs:595-637` |
| `Takes` | `add() -> TakeHandle` | W | `items.rs:640-655` |
| `TakeHandle` | `info()`, `name()`, `pitch()`, `play_rate()`, `volume()` | R | `items.rs:710-805` |
| `TakeHandle` | `peaks(block_size) -> TakePeakData` | R | `items.rs:726-740` |
| `TakeHandle` | `set_name`, `set_pitch`, `set_play_rate`, `set_volume`, `set_source_file`, `set_color` | W | `items.rs:750-860` |
| `TakeHandle` | `make_active()` (→ `Takes::set_active_take`) | W | `items.rs:824-830` |
| `TakeHandle` | `delete()` | W | `items.rs:833-839` |
| `TrackHandle` | `info() -> Track` (carries `lane_count`, `lane_play_mask`, `lane_names`, `lane_display`) | R | `crates/daw/control/src/tracks.rs:325` |

Nothing on `TrackHandle` or `ItemHandle` touches lanes (`grep -n -i lane crates/daw/control/src/tracks.rs` is empty; `items.rs` has no lane method). The proto-level `Takes` trait — `crates/daw/proto/src/take/service.rs:14-146` — is the full write surface: `get_takes`, `get_take`, `get_active_take`, `take_count`, `add_take`, `delete_take`, `set_active_take`, `set_name`, `set_color`, `set_volume`, `set_play_rate`, `set_pitch`, `set_preserve_pitch`, `set_start_offset`, `set_source_file`, `get_source_type`, `get_take_markers`, `add_take_marker`, `set_take_marker`, `delete_take_marker`, `add_take_marker_at_position`, `run_take_rating_action`. daw-control does not yet wrap the take-marker/rating methods or `set_preserve_pitch`/`set_start_offset`; a caller reaches those through the `TakesClient` directly or the batch layer (`BatchOp::Take(TakesOp)`, `crates/daw/proto/src/batch/op.rs:30-124`).

## REAPER backend coverage (`features/reaper/daw-reaper`)

**Takes — complete.** `src/take.rs` implements every `Takes` method against the live API: `get_takes` iterates `count_takes`/`get_take` (`take.rs:38-54`); `set_active_take` resolves the take and calls `SetActiveTake` (`take.rs:147-160`); the setters at `take.rs:162-334`; take markers at `take.rs:336-481`; `run_take_rating_action` saves the selection, selects the item, activates the take, fires `Main_OnCommand(command_id)` and restores the selection (`take.rs:483-528`). `media_take_to_take` reads guid, name, vol, playrate, pitch, preserve-pitch, start offset, colour, source path and `GetMediaSourceType` tag (`src/item.rs:1055-1125`); `source_length/sample_rate/channels` stay `None` ("deferred to #25", `item.rs:1101-1104`). The item poller diffs `active_take_index` and publishes `ActiveTakeChanged` (`src/item.rs:169-194, 349-355`). Integration tests: `tests/reaper_takes.rs:50-380` (delete, preserve-pitch round trip, `set_source_file`, take markers, ratings).

**Fixed lanes — read stubbed, write absent.**

- `media_item_to_item` sets `fixed_lane: None` with the comment "I_FIXEDLANE via the live API — not yet wired" — `src/item.rs:561-562`; the poller does the same (`item.rs:270`).
- Both track readers hard-code `lane_count: 0, lane_play_mask: 0, lane_names: Vec::new(), lane_display: default` — `src/track.rs:184-189` ("Fixed lanes via the live API (I_NUMFIXEDLANES / C_LANEPLAYS) — not yet wired through reaper-rs") and `src/sync_api.rs:569-573`.
- The SDK the crate links (pinned `reaper-rs` branch `feat/reaper-763-ruler-lanes`, `Cargo.toml:458-461`) documents everything needed: item `I_FIXEDLANE` ("fine to call with setNewValue") and `B_FIXEDLANE_HIDDEN`, and item-level `C_LANEPLAYS` — `SDK:1966, 1995-1996`; track `I_NUMFIXEDLANES` (settable), `C_LANESCOLLAPSED`, `C_LANESETTINGS` (bitfield: &2 = do not auto-comp new recording, &8 = big lanes, …), `C_LANEPLAYS:N` (0/1/2, settable), `C_ALLLANESPLAY` — `SDK:2227-2231`; `P_LANENAME:n` — `SDK:2880`. None of these are called from `daw-reaper`. The SDK header has no comp-area accessor at all; comp areas are only reachable through the state chunk or actions.
- `tests/reaper_lanes.rs` is an empty target — its header says it covered marker/region ruler lanes and was retired (`reaper_lanes.rs:1-7`).

**Fixed lanes offline (RPP text) — read and write.** The Pro Tools importer builds lane tracks through `dawfile-reaper`'s builder: `default_fixed_lanes()` writes `FIXEDLANES 9 0 0 0 0` (`src/project_import.rs:566-581`), `apply_fixed_lane_settings` sets `LANENAME` and `LANEREC 0 0 0` (`project_import.rs:583-612`), items get `.fixed_lane(l)` (`project_import.rs:761-762, 861`). The inverse (`project_export.rs:3-12, 71-130`) maps lane 0 to the active playlist and lanes 1..N to alternates. Tests at `project_import.rs:2449-2693` and `tests/pt_playlists_to_lanes.rs`.

**Comp areas — none.** No accessor, no chunk parser, no type.

## Standalone backend coverage (`features/standalone/daw-standalone`)

**Takes — complete except actions.** `src/take.rs` implements the whole trait over an in-memory `TakeList { active_idx, takes }` per item (`src/sync/daw.rs:44-45`): `set_active_take` moves `active_idx` and publishes `ActiveTakeChanged` (`take.rs:189-221`); `add_take` appends a default take with a `standalone-take-N` guid (`take.rs:135-160`); take markers are kept in a per-take `Vec<TakeMarker>` in source order (`take.rs:377-497`). `run_take_rating_action` is `Ok(())` and does nothing (`take.rs:499-509`) — there is no action engine to fire the REAPER command IDs, so ratings can be stored as markers via `add_take_marker` but the "rank the active take" verbs are inert. Playback resolves `TakeRef::Active` to `is_active` (`src/take_reader.rs:60-66`).

**Fixed lanes — read from RPP, honoured at render, no write.**

- `src/project_loader.rs:248-294` derives `lane_count` from `LANENAME` token count, falling back to `max(item.lane)+1` when `FIXEDLANES` is present; `lane_play_mask` from `LANESOLO`'s first two 32-bit words, defaulting to lane 0 when lanes are on but `LANESOLO` is absent; `lane_names` from `LANENAME`; `lane_display` from `FIXEDLANES` field 3 (`One`) and bitfield `&8` (`Big`).
- Items get `fixed_lane = rpp_item.lane` only when the track has lanes (`project_loader.rs:432-438`); `rpp_item.lane` itself comes from the `LANE` token or, in every real project, from `YPOS y height` as `round(y/height)` (`features/dawfile/dawfile-reaper/src/types/item.rs:932-951`; the fixture at `features/dawfile/dawfile-reaper/resources/Template-with-takes-lanes.RPP:223, 248, 273` has `YPOS 0.8 0.2 2` → lane 4, etc.).
- The active take of a comped item is resolved `TAKE SEL` → item-level `GUID` → first take with a source (`project_loader.rs:500-535`), because a comped item opens with `TAKE NULL` slots and carries no inline first take (`tests/comped_take_selection.rs:1-40`).
- The renderer skips items whose lane bit is clear in `lane_play_mask` (`src/audio_engine/render/mod.rs:536-542`; snapshot fields at `render/snapshot.rs:117, 192, 636, 702`). `tests/open_kit_project.rs:50-65` asserts a real kit track: `LANENAME 1 2 3` + `LANESOLO 4` → `lane_count 3`, `lane_play_mask 0b100`.
- No `Tracks`/`Items` method writes any of it (same traits as REAPER), and `save.rs` merges runtime state back into a `dawfile_standalone` document whose `TrackNode`/`ItemNode` carry the proto `Track`/`Item` (`features/dawfile/dawfile-standalone/src/document.rs:112-160`) — so lane fields round-trip only as far as that document's RPP exporter writes them, and `src/rpp/export.rs` has no `FIXEDLANES`/`LANENAME`/`LANE` emitter (`grep` hits only `LANEHEIGHT`, `export.rs:996`).
- **Loader disagreement**: `dawfile-standalone`'s own RPP importer (used by `save.rs:49`) reads `FIXEDLANES` field 1 as `lane_count` and field 2 as `lane_play_mask` and `LANENAME` tokens from index 2 (`features/dawfile/dawfile-standalone/src/rpp/import.rs:304-310`), whereas `dawfile-reaper` reads field 1 as a settings bitfield, field 2 as `allow_editing`, and every `LANENAME` token as a name (`features/dawfile/dawfile-reaper/src/types/track.rs:860-889`). On the fixture's `FIXEDLANES 9 0 0 0 0` / `LANENAME "Custom Lane Name" C1 1 2 3` the first yields 9 lanes, mask 0, names `["1","2","3"]`; the second yields 5 lanes, mask from `LANESOLO`, five names. The REAPER SDK's `C_LANESETTINGS` bit meanings (`SDK:2229`) match `dawfile-reaper`'s reading (bit 8 = big lanes, which `project_loader.rs:287-294` also uses).

**Comp areas — none**, and the RPP records are dropped: `dawfile-reaper`'s track parser lists its known tokens (`types/track.rs:676-706`, includes `FIXEDLANES`/`LANEREC`/`LANENAME`/`LANESOLO`, not `ITEMLANES`/`LINKEDLANE`) and its per-line match ends in `_ => {}` (`types/track.rs:996`); `Track` has no verbatim-extra-lines field, so `ITEMLANES 5` and `LINKEDLANE 2 5.205 4 0 -1 0.01 0.01` (`Template-with-takes-lanes.RPP:209-211`) are lost on read and absent on write. `LANEREC -1 0 1` (record lane, comping lane, last comping lane) is parsed into `LaneRecordSettings` (`types/track.rs:343-347, 869-874`) and serialised back (`types/serialize.rs:793-797`) but never mapped into the proto.

## Gaps

| capability | REAPER | standalone | what would need adding |
|---|---|---|---|
| List takes, active take, add/delete/activate | yes (`take.rs:38-160`) | yes (`take.rs:80-221`) | — |
| Take name/colour/vol/rate/pitch/offset/source | yes (`take.rs:162-334`) | yes (`take.rs:223-375`) | daw-control wrappers for `set_preserve_pitch`/`set_start_offset` |
| Take source length / rate / channels | `None` (`item.rs:1101-1124`) | from decoded source | `PCM_source_GetLength/SampleRate/NumChannels` wrappers in daw-reaper |
| Take markers CRUD | yes (`take.rs:336-481`) | yes, in memory (`take.rs:377-497`) | standalone: persist markers in the save document |
| Take ratings (rank actions) | yes via `Main_OnCommand` (`take.rs:483-528`) | no-op (`take.rs:499-509`) | standalone: implement rank as marker writes using `TakeRating::to_marker_name` |
| `ActiveTakeChanged` event | yes, polled (`item.rs:349-355`) | yes, on set (`take.rs:213-220`) | — |
| Read `Track.lane_count/lane_names/lane_display` | **no** — zeros (`track.rs:184-189`, `sync_api.rs:569-573`) | yes from RPP (`project_loader.rs:248-294`) | daw-reaper: `I_NUMFIXEDLANES`, `P_LANENAME:n`, `C_LANESETTINGS` via `GetSetMediaTrackInfo` (needs reaper-rs wrappers) |
| Read `Track.lane_play_mask` | **no** | yes (`LANESOLO`) | daw-reaper: loop `C_LANEPLAYS:N`; standalone: fix the `dawfile-standalone` importer to match `dawfile-reaper` |
| Read `Item.fixed_lane` | **no** — `None` (`item.rs:561-562`) | yes (`project_loader.rs:432-438`) | daw-reaper: `I_FIXEDLANE` |
| Write lane count / names / play mask / display | **no** service method | **no** service method | new `Tracks` methods (`set_lane_count`, `set_lane_name`, `set_lane_plays`, `set_lane_display`); REAPER impl via the settable `I_NUMFIXEDLANES`/`C_LANEPLAYS:N`/`C_ALLLANESPLAY`; standalone impl over `Track` + save-document export |
| Move an item to a lane | **no** | **no** | `Items::set_fixed_lane`; REAPER via settable `I_FIXEDLANE`; standalone sets the field and the renderer follows |
| Record / comping lane (`LANEREC`) | parsed offline only (`types/track.rs:869-874`) | parsed offline, not surfaced | proto fields `record_lane`, `comp_lane` on `Track` + a REAPER reader (state chunk; no SDK accessor found) |
| Comp areas (`ITEMLANES`/`LINKEDLANE`) | **none** — dropped by the parser (`types/track.rs:996`) | **none** | proto `CompArea { start, end, comp_lane, source_lane, fade_in, fade_out }`; RPP parse + serialise in `dawfile-reaper`; REAPER live read via `GetTrackStateChunk`, write via chunk splice (the SDK has no accessor); standalone: build the comp lane's items from the areas |
| Lane / comp events | **none** | **none** | `TrackEvent::LanesChanged`, `ItemEvent::LaneChanged`, `CompAreasChanged` on the bus |
| Persist lanes through standalone save | n/a | **no** (`rpp/export.rs` writes no lane tokens) | emit `FIXEDLANES`/`LANESOLO`/`LANENAME`/`LANEREC` + per-item `YPOS` from the proto fields (the serialiser at `types/serialize.rs:550-554` notes REAPER rejects a `LANE` token in an `<ITEM>`; `YPOS` is the wire form) |

## Recommendation for the comp model in the window

Model the comp in the session window on top of what is portable today and do not wait for REAPER's comp areas: a **take lane** is `(track, fixed_lane)` and a **take** on it is the item(s) whose `Item.fixed_lane` equals that lane, with `Track.lane_names[lane]` as the label and `Track.lane_play_mask` as "this lane is the one you hear"; the **comp lane** is one more fixed lane whose items are ordinary items with `Take.start_offset` and `source_file_path` copied from the chosen take region and item fades as the crossfades (`flow.drums.comping.crossfade`) — exactly the shape REAPER itself writes for a comped lane (fixture items at `Template-with-takes-lanes.RPP:368-387`: an item on the comp lane whose `SOFFS 3.008` points into the source take), so the same data is correct on REAPER and standalone and needs only `Items::set_fixed_lane`, `Tracks::set_lane_plays` and the REAPER lane readers from the gap table. Keep the FTS-side **comp region list** (which source lane each stretch of the comp came from, the group membership across the kit's tracks for `flow.drums.comping.group`, the per-language split for `flow.vocals.comping`) as session data in ext-state / the side store rather than as REAPER `LINKEDLANE` records, and treat `LINKEDLANE` as an *export* target to add to `dawfile-reaper` later so REAPER's own comp tools see the same areas; that keeps the window's comp editable when the backend is standalone, where no comp-area concept exists, and keeps the kit-wide group choice — which REAPER has no notion of — in the one place that owns it.
