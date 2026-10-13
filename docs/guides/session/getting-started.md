# Session — Song Library & Setlists

Session's job is to make a band's backing-track library and setlists easy
to create, share, and eventually play — synced across Signal (guitar/keys
rigs), other Session instances, and Ignition (lighting). This is the first
slice: turning a raw folder of stems into a real, portable Song Library,
and setlists that are just markdown with `[[wikilinks]]`.

## `session-vault-sync`

`apps/vault-sync` (crate `session-vault-sync`) scans a Tracks folder — by
default `/home/cody/Task/days-to-praise/Assets/Tracks`, one folder per
song named `"{Title} - {Artist}"` (no key — that's tag data, not a
filename) — and writes a `type: song` note **directly into that song's
own folder**, alongside its audio/lyrics/chart. Deliberately not written
into Task's internal `.task/orgs/*/vault` storage: the point is a Tracks
song folder is one self-contained, portable unit — copy or zip it and
the note travels with everything it describes. Setlist notes go to a
separate `setlists_dir`, default `/home/cody/Task/days-to-praise/Assets/Setlists`
— same Assets tree, not the internal vault either.

Each song folder now looks like:

```
Praise - Elevation Worship/
  audio/
    ogg/       — one .ogg per stem, preferred (small, streamable)
    wav/       — the source .wav per stem
    reference/ — a reference mix (click/cue/master), not multitrack stems
  lyrics/      — {slug}.json: artist/key/sections tag data
  chart/       — {Title}.kf: a real keyflow chart, when transcribed
  sync/        — {slug}.kf.json: forced-alignment word timing, if generated
  {Title} - {Artist}.md   — the type: song note, from sync-library
  {Title} - {Artist}.RPP  — the organized REAPER project, from build-project
```

Stems are read from `audio/ogg/` if present, else `audio/wav/` (older
folders not yet migrated to the `audio/` nesting fall back further, to a
bare `ogg/`/`wav/` sibling or loose `.wav` files — see `find_stems` in
`src/library.rs`). Key comes from `lyrics/*.json`
(`{title, artist, key, sections: [{label, lines}]}`) — a song with no
`lyrics/` file just has no key yet, which the generated note leaves
absent rather than guessing. The generated note's `## Chart` section
comes from `chart/*.kf` when one exists — the real keyflow chart syntax
(`session::setlist::chart_import` already parses this for accurate
tempo/time-signature/section lengths); `## Lyrics` is always the plain
lyric text from `lyrics/*.json` when present, alongside the chart rather
than instead of it. All six songs currently in the library have real
`.kf` charts, lifted verbatim from the demo charts already bundled in
`crates/session/session/src/setlist/service/demo.rs` (`PRAISE_CHART` and
siblings) — that file happened to already have accurate transcriptions
for exactly these six songs.

```bash
cargo run -p session-vault-sync -- list-songs         # preview, writes nothing
cargo run -p session-vault-sync -- sync-library        # writes a .md into each song's own folder
cargo run -p session-vault-sync -- create-setlist "Sunday, August 30 2026" \
    "Thank God I'm Free" "Washed" "Holy Forever"       # writes Assets/Setlists/…
cargo run -p session-vault-sync -- build-projects      # writes a real .RPP per song (see below)
```

A generated song note (`Tracks/Holy Forever - Bethel Music/Holy Forever - Bethel Music.md`):

~~~markdown
---
type: song
artist: Bethel Music
key: Bb
stems:
  - name: "AG"
    path: "audio/ogg/Holy Forever - AG.ogg"
  - name: "Bass"
    path: "audio/ogg/Holy Forever - Bass.ogg"
  ...
---
# Holy Forever

Bethel Music — Bb

## Chart

```keyflow
Holy Forever - Bethel
72bpm 4/4 #Bb
...
```
~~~

A generated setlist note:

```markdown
---
type: setlist
title: Sunday Set
---
# Sunday Set

[[Thank God I'm Free - Elevation Rhythm]]
[[Washed - Elevation Rhythm]]
[[Holy Forever - Bethel Music]]
```

Song notes are filed as `"{Title} - {Artist}.md"` (matching the Tracks
folder naming exactly), not just `"{Title}.md"` — two different artists'
songs sharing a title would otherwise collide and silently shadow each
other in Task's link resolution. `create-setlist` accepts either form on
the command line (a bare title, or the full `"Title - Artist"`), resolves
it against the library, and always writes the disambiguated
`[[Title - Artist]]` form into the note; it errors out if a bare title is
ambiguous between two songs rather than guessing.

This deliberately matches the Task app's own note format byte-for-byte —
specifically `setlist_songs_from_body` in the Task repo's
`crates/ui-core/src/frontmatter.rs`, which reads a `type: setlist` note's
songs as plain standalone `[[Title]]` lines in the body, in document
order. Nothing on the Task side needed to change for these notes to
parse; you can hand-edit a setlist note in Task directly (reorder lines,
add a song by typing `[[Song Title]]`) and it round-trips.

## Real REAPER projects: `build-project` / `build-projects`

`rpp.rs` builds an actual `.RPP` per song — one track per stem, run
through **dynamic-template's real organize pipeline**
(`apply_colors` → `apply_buses` → `apply_routing` → `gather_unsorted`,
the exact sequence `dynamic-template --apply-buses` runs, called
in-process against the same public API, not reimplemented). The result
groups stems into instrument buses (DRUM BUS, BASS BUS, GUITAR BUS with
nested ACOUSTIC/ELECTRIC, KEYS BUS, FX BUS, VOX/BGV BUS when vocals are
present, …), routes each stem to the bus its name classifies into, and
colours tracks by classification — reading like a hand-built session,
not a flat stem dump. Reference shape: `Thank God I'm Free - Elevation
Rhythm/Thank God I'm Free - Elevation Worship.RPP`, a real 36-track
project built by hand for that song — `build-projects` produced the same
kind of artifact (36 tracks: 23 stems + 13 buses) for `Praise` from
nothing but its stem folder.

```bash
cargo run -p session-vault-sync -- build-project "Holy Forever"  # one song
cargo run -p session-vault-sync -- build-projects                # every song in the library
```

Written as `"{Title} - {Artist}.RPP"` into the song's own folder, source
paths relative to the RPP (`audio/ogg/…`) so the folder — project
included — stays portable. Item length comes from the real stem
duration: `wav_duration_seconds` reads just the WAV header's `fmt `/
`data` chunk sizes (never the audio payload) off the `audio/wav/`
sibling, even when the item itself points at the `.ogg` copy, since
probing OGG duration needs a real decoder and a WAV header doesn't.
Tempo/time-signature come from the song's `chart/*.kf` first line when
present (`72bpm 4/4 #Bb` et al.), defaulting to 120bpm 4/4 otherwise.

This is a generation step, not a live player — see `apps/session-player`
below for that.

### The live scheme: `--live` / the Master Setlist Template

`build-project`/`build-projects` also take `--live`, which builds a
completely different, second scheme instead: the **Master Setlist
Template** (`live_bus.rs`) — a fixed, purpose-built live-FOH bus layout
every song's project conforms to, as opposed to dynamic-template's
general studio taxonomy above. Deliberately a separate, from-scratch
classifier and tree-builder in this crate, not a variant bolted onto
`dynamic-template` itself — other things depend on that crate's existing
bus tree, and this one is a small, worship-live-rig-specific taxonomy:

Two separate sections, not a bus-tree-with-content-nested-inside-it:
raw stems are sorted into their own content folders by instrument, and
one flat `BUSES` folder holds every live bus, each fed by a send
(`AUXRECV`) from its content tracks — not physically containing them.
That's the shape a hand-built live session actually takes: scroll the
content folders to tweak a mic, work the `BUSES` folder to mix.

```
Content (each its own top-level folder):
  CLICK + CUES  ← Click, Guide, Count, Cue
  DRUMS         ← Drums, Percussion, Perc, Hand Percussion
  BASS          ← Bass, Electric Bass, Synth Bass, Upright Bass
  GUITARS
  ├── ACOUSTIC  ← AG, Acoustic Guitar
  └── ELECTRIC  ← EG, Electric Guitar
  KEYS          ← Piano, Keys, Organ, Rhodes, Wurlitzer, Clavinet
  SYNTHS
  ├── LEADS     ← Synth Lead only (not lead guitar/sax/keys — those stay on their own instrument folder)
  └── PADS      ← Synth Pad only
  ORCH
  ├── STRINGS   ← Strings, Fiddle, Harp
  ├── WOODWINDS ← (none in this library yet) + Harmonica
  ├── BRASS     ← Saxophone, Trumpet, Trombone, "Horns" generally
  └── PERCUSSION ← orchestral percussion (distinct from the drum-kit DRUMS folder)
  FX            ← Synth FX, FX 1/2, Vox FX, SFX, Loop, Bells, Arps, anything unrecognized
  VOX
  └── BGV       ← BGVS, Choir, BG Harm A–E (no Lead Vox — no backing-track stem ever fills it; the live singer is separate)

BUSES (one flat folder, fed by sends from the content tracks above):
  CLICK + CUES BUS, DRUMS BUS, BASS BUS, ACOUSTIC BUS, ELECTRIC BUS,
  KEYS BUS, LEADS BUS, PADS BUS, STRINGS BUS, WOODWINDS BUS, BRASS BUS,
  ORCH PERCUSSION BUS, FX BUS, BGV BUS
```

```bash
cargo run -p session-vault-sync -- build-project "Holy Forever" --live   # → "Holy Forever - Bethel Music (Live).RPP"
cargo run -p session-vault-sync -- build-projects --live                 # every song, same scheme
```

`classify(label: &str) -> LiveBus` (`live_bus.rs`) now runs
dynamic-template's **real** classifier first (`real_classify` —
`monarchy::Parser::new(&dynamic_template::default_config())
.parse(label)?.metadata.group`, the same parser/taxonomy
`organize_into_tracks` and the studio scheme use), mapping its
classification path to a `LiveBus` via a small table (`"Guitars"` +
`"Acoustic"`/`"Electric"` → Acoustic/Electric, `"Synths"` +
`"Lead"`/`"Pad"` → Leads/Pads, etc.). A tiny keyword fallback
(`fallback_classify`, just `Fx`) only covers what the real classifier
still leaves unclassified — the documented "KNOWN GAP"s in
dynamic-template's own tests (bare "Arps", bare "Synths").

Every song in this library already has a passing test proving this
classification is correct:
`features/dynamic-template/tests/multitrack_examples/{holy_forever,
thank_god_im_free, washed, who_else, god_im_just_grateful,
elevation_worship_praise}.rs` — real stem filenames, asserting which
top-level group (`Guide`, `Bass`, `Guitars`, `Keys`, `Synths`,
`Vocals`/`Choir`, `Orchestra`, …) each one lands in. That's where to
look to customize the classification itself (add a pattern, fix a
misclassification) — this crate only maps dynamic-template's group
paths onto the eleven live buses, it doesn't reimplement the parsing.

**Why not just use `organize_into_tracks`'s rendered tree directly?**
It collapses a folder away entirely when it would hold only one child —
confirmed with `cargo run -p dynamic-template -- -v "... Synth Pad"`:
a lone "Synth Pad" renders as a bare `Pad` track with no enclosing
folder at all ("only create folders when needed to organize multiple
things" — `dynamic-template`'s own stated philosophy, see
`tests/01_no_unneccesary_folders.rs`). This scheme wants Pads to always
be its own folder/bus, one stem or ten, so `build_content_tree` uses only
the *per-item classification path* (`ItemMetadata::group`, unaffected by
that collapse) and builds its own folder tree from it — every non-empty
`LiveBus` category gets a folder regardless of how many stems land in it.
Verified: Thank God I'm Free (one Synth Lead stem, one Synth Pad stem)
produces real, separate `LEADS` and `PADS` folders and buses in the
generated project, each holding exactly its one stem.

Naming and color, both explicit asks: every content track and its item
are named just the stem label ("AG", "EG 1", "Synth Pad" — no song-title
prefix baked in, since the project itself already carries the song's
name). Every track — content, its folder, and its bus alike — gets a
consistent color per category via `LiveBus::color()`, pulled from the
**established palette** `dynamic-template`'s own `apply_colors` already
uses for the studio scheme (`dynamic_template::colors::{groups, guitars,
synths, vocals}`, backed by `music_catalog::instruments` — Tailwind-based
hex values, e.g. `groups::DRUMS` is red-500 `#ef4444`) — so a Drums
track here is the exact same red as a Drums track in the studio scheme.

Getting the actual on-disk encoding right took two passes. First pass
invented its own RGB triples with the wrong byte order. Second pass
fixed the palette source but delegated the encoding to
`dynamic_template::colors::to_reaper_color`, which turned out to be
**platform-conditional** underneath
(`color_palette::Color::to_reaper_native`: `#[cfg(target_os =
"windows")]` → BGR, every other target → raw RGB) — wrong on its face,
since `.rpp` is a portable text format REAPER reads identically
regardless of which OS wrote it; there is no such thing as a
build-platform-dependent on-disk color encoding. Building on Linux hit
the "raw RGB" branch. The actual, correct, non-conditional format is the
one `dawfile-reaper`'s own `marker_with_color` already documents:
`0x01000000 | (b<<16)|(g<<8)|r` (BGR) — `live_bus.rs` now builds this by
hand (`reaper_color(r,g,b)`, pulling raw `.r()`/`.g()`/`.b()` off the
`Color` value) instead of trusting `to_reaper_color`. Verified against
cyan-400 (`34, 211, 238` — R/G/B all different, so a byte-order bug is
actually visible, unlike red-500 where G and B happen to match):
generated `PEAKCOL 32428834` decodes back to exactly `(34, 211, 238)`
under this formula. Folder-marker tracks (the content folders,
GUITARS/VOX branch folders via a `branch_color` fallback onto
`groups::GUITARS`/`groups::VOCALS`, and the outer `BUSES` folder) now
get `.color(...)` too — the first pass only colored leaf stem tracks and
left every folder uncolored.

**Orchestra colors are a real source fix, not a local override.** The
default palette had `groups::ORCHESTRA` as purple and `orchestra::
{STRINGS, WOODWINDS, BRASS}` as amber/sky/amber — changed at the source
in `music-catalog` (`/run/media/Development/music-convention`,
`instruments.rs`) to `stone::S600` (brown) / `green::S600` / `blue::S500`
/ `yellow::S500` respectively (`orchestra::PERCUSSION` was already
`orange::S600`, left as-is). Picked up here via a local `[patch]` in the
root `Cargo.toml` (`music-catalog` **and** `color-palette` — patching
only the former left two versions of `color-palette` in the dependency
graph and a type-mismatch build error) — per this repo's own documented
convention for cross-repo co-development, **never commit that patch
block**; it's a machine-specific path. The real fix belongs in a
`music-convention` release once verified. Since Orch content now
actually needs to show 4 distinct colors, it split from one flat ORCH
bus into a real branch: `ORCH` (brown) containing `STRINGS`/`WOODWINDS`/
`BRASS`/`ORCH PERCUSSION` sub-buses, matching dynamic-template's real
`["Orchestra", "Strings"]`-shaped classification paths — "Horns" (its
own top-level group covering Saxophone/Trumpet/Trombone) folds into
Brass, "Fiddle" into Strings, "Harmonica" into Woodwinds as the closest
available fit — none of which have real stems in this library yet, so
those foldings are untested guesses, easy to revisit in `real_classify`.

Structurally: `build_content_tree` buckets stems by [`classify`] and by
[`LiveBus::content_path`] (which folder segment(s) a bus's content lives
under — most are one top-level folder, Acoustic/Electric nest under
GUITARS, BGV nests under VOX); `emit_content` flattens that tree
depth-first with the same closing-level bookkeeping the earlier
bus-nested version used (every node adds one closing level on top of
whatever its parent passes down, so e.g. `ELECTRIC`'s last track closes
both Electric's and Guitars' folders in one step when Electric is
Guitars' last populated child), while recording each stem track's final
index. `emit_buses` then opens one `BUSES` folder and, for every bus
that got at least one stem, adds a plain (non-folder) bus track with a
`.receive(index)` call per recorded content-track index. Content tracks
get `master_send = Some(MasterSendSettings { enabled: false, .. })` —
set directly on the built `Track` (there's no `TrackBuilder` method for
it) since they should reach the mix only through their bus, never
directly. Verified across all six songs, including the different bus
combinations each one actually exercises (Praise reaches `VOX`/`BGV`;
Washed reaches `ORCH` via Saxophone; Holy Forever has neither, and the
file still closes cleanly).

Not decided yet: whether `--live` should be the default for
`build-projects` once it's proven out, and whether `session-player`
should switch from its current flat per-stem seeding to this same
content/bus split (so the live player and the live `.RPP` agree on
grouping) — today they're independent.

The `stems: [{name, path}]` list is **not** part of Task's existing
schema — it's an extra, harmless field Task's hand-rolled frontmatter
reader (`frontmatter_value`/`front_block_maps`) simply ignores today. It
exists so a human — or a future importer — can find the real audio;
`path` is relative to the note's own folder (`audio/ogg/...`), which is
exactly where it is since the note lives inside that same folder.
`key:`/`artist:` are real fields Task's `SongFront` reader already knows.

## The live player: `apps/session-player`

`session-player` (crate `session-player`) plays a setlist for real —
natively, no REAPER, no browser. It's driven by `daw-standalone` directly
(the same engine `apps/desktop/src/session_engine.rs` uses for
its in-process setlist player), not the RPC/vox facade that crate builds
for remote UIs — this is a local CLI, so it calls `Standalone`'s sync
`Transport`/`Tracks` trait methods directly.

```bash
cargo run -p session-player -- --setlist "Assets/Setlists/Testing/Sunday, August 30 2026.md"
# or, for a quick test without a setlist note:
cargo run -p session-player -- "Thank God I'm Free" "Washed" "Holy Forever"
```

What it does: reads the setlist note's `[[Title - Artist]]` links (same
parser shape as Task's `setlist_songs_from_body` — plain wikilinks, alias
and `#anchor` forms stripped), resolves each against the Tracks folder,
then seeds one `daw-standalone` project per song straight from its stems
(`daw_standalone::media_seed::seed_media_tracks` — one track per stem,
no dynamic-template grouping for the player; that's a `.RPP`-generation
concern, not a live-transport one). Item length is the same
`wav_duration_seconds` header probe `rpp.rs` uses. Only the *current*
song's audio engine is attached (real cpal output) at a time — matching
the "one render graph at a time" design Task's own aspirational browser
player already settled on for the same reason.

A tiny stdin REPL drives it: `n`/`p` to switch songs (stops the old
engine, attaches a fresh one to the next project), `pl` to play/pause,
`s` to stop, `list` to see the current song's tracks, `mute`/`unmute`/
`solo`/`unsolo <name>` matching any track whose name contains `<name>`.
Confirmed working end-to-end against the real six-song library: loading,
switching songs mid-session, and mute/solo all functioned with zero
errors — including on a song with 36 stems (`Thank God I'm Free`).

One implementation note worth keeping: `daw-standalone`'s seeding/
audio-engine calls spawn background tasks via `architect::platform::spawn`
internally even though the surface API is plain sync — the binary panics
("no reactor running") without an ambient Tokio runtime present, so
`main` is `#[tokio::main] async fn main()` even though nothing in it ever
actually `.await`s anything.

Also worth knowing before extending it: `daw_proto::Transport` names both
the RPC trait and an unrelated plain data struct (transport *state*), at
the same top-level path — the struct's named re-export silently shadows
the trait's glob one there, so `use daw_proto::Transport as _` resolves
to the struct and the compiler reports the trait's own methods as
missing. The escape hatch is deliberate and documented in `daw-proto`
itself: import from `daw_proto::transport::service::Transport` instead,
which stays a `pub mod` specifically so the trait remains reachable.

Not built yet: volume control (mute/solo cover the primary rehearsal
need — isolate or drop a part — for now), and reading a `.RPP`'s track
layout back into `Standalone` instead of re-deriving it from the stem
folder (would let the player and `build-project` agree on grouping).

## What's deliberately not built yet

Investigated in depth before writing any of this (see the research this
guide is a record of), so the gaps are known rather than accidental:

- **No content is uploaded or served, and Task doesn't index this folder
  yet.** Task's real playback path needs either uploaded, content-hashed
  blobs (`stems: [{content_hash}]`) or a colocated `/media/songs/{slug}/…`
  folder the Task server resolves and signs a grant for — neither exists
  for these songs. And separately: Task's vault for this org is actually
  rooted at `/home/cody/.task/orgs/days-to-praise/vault` (confirmed via
  `org.toml`/`.fts-root.json`), NOT `/home/cody/Task/days-to-praise/`, so
  a `[[Title - Artist]]` link in a setlist note won't resolve inside the
  live Task app until the org's vault is pointed at (or made to include)
  this Assets folder. That's a Task-side config/wiring question, not
  something this tool can fix by itself.
- **No public/anonymous share link yet.** Task's `ShareService` is real
  and wired (`features/share/share-proto`, `apps/server/src/share.rs`)
  and already supports `ShareTarget::Note`, but the Note landing path
  today deep-links into the *authenticated* app shell with chrome hidden
  — not a genuinely public render. The one target with real anonymous
  access is `ShareTarget::Review` (`apps/server/src/share_guest.rs` — a
  scoped guest vox lane, no login, the token is the whole grant). That's
  the pattern to extend for a setlist, whenever `session.fasttrackstudio.app`
  is built: either a new `ShareTarget` with its own guest lane, or a
  dedicated unauthenticated HTTP path patterned on
  `share_rendition_handler`/`share_download_handler`.
- **No web/GUI player yet** — `apps/session-player` (above) is a
  terminal REPL, which is enough to prove the whole chain end-to-end
  (setlist → resolved songs → seeded projects → real synced audio →
  mute/solo) but not what a musician wants open during rehearsal. Decided
  direction (from the person driving this): everything, including the
  eventual web interface, is built in *this* repo, not Task's — Task
  later pulls session's web interface in, the same direction this repo
  already takes `daw`/`keyflow`/`song` from itself and hands
  `editor`/`collection-proto` back the other way. Task's own
  `crates/player-ui` (a browser multitrack player on `daw-standalone` +
  `AudioWorklet`, real but never exercised with real data) is fair game
  to lift code from as a starting point, not to build against as a
  dependency.
- **No roster/role assignments** ("who's playing drums this week"). Task
  has no team-scheduling model yet (checked `features/scheduling/` in the
  Task repo — it has cal.com-style bookings and a plain calendar, neither
  of which fits). Deferred by explicit choice; the setlist note format
  above has room for it later (e.g. a `team:` block) without a rewrite.
