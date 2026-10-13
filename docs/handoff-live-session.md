# Handoff: the live Session — charts, lyrics, and the Overview

Written 2026-09-23, replacing the 2026-09-22 version of this file (which
covered the setlist, Organize mode and live keyflow; what is still true of
it is kept below). Everything is **committed locally only** — nothing
pushed, merged or deployed, by the user's standing rule (commit locally;
push once the WASM demo is right). Six sibling checkouts under
`/Volumes/build-disk/development/` are in play, and the session app builds
**four of them from local paths** (§8 — they must be pushed and pinned
before session is pushed).

## 1. What we are building, in the user's words

Session is the live player and the DAW: a setlist of songs that plays
itself on a service, and the arrangement/mixer/chart to prepare them with.
Keyflow (`.kf`) is the song's chart and the source of its structure —
tempo, meter, sections, key, chords — and from them the click, count and
spoken cues. Lyrics are layered — Song, Section, Slides, Lines, Words,
Syllables, Syllables + melody — and a deeper layer gives every layer above
it. Prepare a song **once**, save it as a `.session`, open that from then
on.

## 2. Run it, and see what it did

```bash
cd /Volumes/build-disk/development/session
FTS_SESSION_VIEW=overview just app "../sessions/Worship Set.setlist" "" live   # the set, Overview, Live mode
FTS_SESSION_VIEW=overview just app "../sessions/Worship Set.setlist" "" organize
just app "../sessions/imported/God, I'm Just Grateful/God, I'm Just Grateful.RPP"   # one song (quoting handles the ')
just prepare ../sessions/imported/*/*.RPP        # prepare songs into .session ahead of time
FTS_SESSION_REPREPARE=1 ./target/release-fast/prepare "<Song>.RPP"   # re-prepare one (after a chart or .lrc change)
cargo run -p session-cli -- lyrics fetch <Song>.RPP [--pick N --force]  # synced lyrics → <Song>.lrc
```

- `just app` builds `release-fast`, wraps the binary in
  `target/release-fast/Session Dev.app`, **signs it with the Apple
  Development identity and a fixed identifier** (so macOS keeps its
  removable-drive grant across rebuilds — the user confirmed no more
  prompts), and opens it in front. Env passed through: `FTS_SESSION_VIEW`
  (overview/performance/setup), `FTS_LYRICS_VIEW` (audience/performer/
  confidence), the MODE argument (live/organize/…).
- **Logs:** `~/Library/Logs/Session Dev/session-dev.log` (tracing, panics
  with backtraces). The file appends across runs: find the *latest*
  `SWELL API provider not found` line (one per launch) before reading a
  panic. "It crashed" → read this first.
- **Seeing the window:** you cannot click or type into it — interaction is
  the user's, or a test's. Screenshot it by window id:

  ```swift
  // winid.swift — usage: swift winid.swift <pid>
  import CoreGraphics
  import Foundation
  let pid = Int32(CommandLine.arguments[1])!
  let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as! [[String: Any]]
  for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == pid {
      print("\(w[kCGWindowNumber as String]!) name=\(w[kCGWindowName as String] as? String ?? "")")
  }
  ```

  ```bash
  pid=$(pgrep -f "Session Dev.app/Contents/MacOS/session-desktop" | head -1)
  wid=$(swift winid.swift $pid | grep "name=Session" | awk '{print $1}')
  screencapture -x -o -l $wid out.png   # 2560x1320; crop with sips -c H W --cropOffset Y X
  ```

- **Measuring the live layout** (when a pane looks wrong only in the
  window — see §7): a temporary `onmounted` + `MountedData::get_client_rect()`
  logged with `tracing::info!` twice a second, read from the log, then
  deleted. Headless probes did NOT reproduce the one layout bug that
  mattered. Never call `get_scroll_size()` from a `use_future` —
  it panicked ("RefCell already mutably borrowed").

## 3. Where it stands — done in this session (2026-09-22/23)

### Charts (keyflow) and the songs' `.kf`

All seven imported songs + the curated `sessions/Always On Time` have
their **real chords** in their `.kf` now (the user dictated rhythms; old
charts kept beside them as `<Song>.kf.before-chords`):

| song | notes / guesses to confirm |
|---|---|
| God, I'm Just Grateful | CH 2A, 3A, 3B end on 5–4; CH 2B → 1/3 2m7; CH 3C "Alt" ends N.C. (`r r`); pushes `1maj7 /// 5 /` etc. |
| Holy Forever | Pre 1 has no passing 6; Pre 2 ends 42; Turn removed; Tag 2 = "We'll sing…amen" line (guess); CH B's 1/3 pickup on beat 3 of the previous bar (guess) |
| Thank God I'm Free | CH = first half, Post = second half; Bridge 2/Tag B/D# as a 2-beat pickup (guess); Chorus 2 = 6 bars N.C. (`r1`) then A5 G#5 E F#m |
| Washed | intro/verse `1/3 / 4 /// '5/7 / 6m ///`, verse ends `2m //// '5sus ////`, chorus `1 '4 6m '5sus`; bridge/refrain 2 bars a chord (guess) |
| Who Else | INST = 42 5 42 5; three bridges (BR C is 4 bars: 42 5 6m7 5); bar splits are guesses |
| Always On Time, Praise | charts unchanged (they had chords) |

Keyflow language/engine changes (keyflow + daw's keyflow-proto):
- `42` / `4:2` / `G2` = add2, displayed `4add2`; `add4` stays `add4`;
  `5(add4)` parses (a parenthesised addition belongs to its chord — the
  chart parser used to read it as a rhythm group and leak a `_2` length).
- **Chord memory is OFF by default**; `/CHORD_MEMORY=true` under the header
  turns it on. Explicit assignments (`Cm = Cm7b5`) still apply.
- Bare `r` / `s` = a bar of rest/space in place (was dropped + padded at the
  end). Bare `PRE`/`Post`/`Intro`/`Outro` headers replay (were excluded). A
  chart may open on a counted `PRE`/`Post`. A bare header replays the
  **most recent** writing of that section.
- The engraver's playback cursor keeps time at each measure's own meter
  (the 2/4 Breakdown bug).
- Chord track: a chord ends at its bar line; rests make no item; **pushes
  (`'4`) sound an eighth early** on the CHORD track.

### The ruler's CHORDS lane, and the Keyflow folder

- Chord/key names are item **labels** (REAPER `P_NOTES`), and the
  `.session` round trip dropped them → blank CHORDS lane. Fixed in
  daw-standalone (loader reads `<NOTES>`, writer writes it; an empty take
  is added for take-less KEY items). All songs re-prepared.
- The Keyflow folder is **visible** now, with KEY, CHORD, LINES, HITS hidden
  (`prepare` hides them); only its new **Lyrics** track shows. The user
  wants the chords on the ruler lane, not as tracks. **LINES is for
  melodies** — never put lyrics there.

### Lyrics (new)

- **Model** — `crates/session/session/src/lyrics.rs`: `Layer` (7 layers),
  `Lyrics { lines }` with `Line/Word/Syllable` (seconds + optional pitch),
  `from_lrc`, `deepest()/layers()`, `sections(&[SectionSpan])` (a line
  belongs to the section it overlaps most), `slides(sections, 2)`,
  `from_items`, `stamp_lines` (onto the Lyrics track, created before HITS
  in the Keyflow folder), `Anchor` + `align`.
- **Fetch** — `session lyrics fetch` (apps/cli/src/lyrics/): LRCLIB (no
  auth), provider trait with room for Spotify/Deezer/Musixmatch; title +
  artists from the `.kf` header; picks the version nearest the song's
  SONGSTART→SONGEND length (`--pick N` to override). All 8 songs have a
  `<Song>.lrc`. Doubtful picks: **Holy Forever** (nearest recording is 82 s
  shorter), **Praise** (radio edit), **Always On Time** (credited "feat.
  Bella Cordero"). The user pointed at
  github.com/tappyduckmancodes/multiplatform-lyric-downloader as the model.
- **Anchors** — each `.lrc` carries `[#anchor: <section> | <beats> | <line> |
  drop-before]`; prepare resolves the section to its region's downbeat
  (`VS 1` also matches `VS 1A`), adds beats at the song's tempo, shifts
  every line, optionally drops lines before. Set per the user:
  Always On Time `VS 1 | -1`, Grateful/Holy Forever/Praise/Free `VS 1 | 0`
  (Free drops 3 ad-libs), Washed `VS 1 | -1` (drops the opening chorus),
  Who Else `CH 1 | -2 | Who else is worthy`. One anchor = one global shift;
  lines drift where the recording's arrangement differs.
- **Panel** — `apps/session-daw/src/lyrics_panel.rs`, in the Overview
  under the chart (and in the Performance view beside it). Three views:
  **Confidence Monitor** (default; ProPresenter stage display: current
  slide white, rule, next slide in its **section colour**, yellow if the
  colour is whitish; rotated section-name rails down the left of each
  half; the section strip along the bottom), **Performer** (one header
  line — section chip, progress, "next in Ns" — over a self-scrolling
  teleprompter of the whole song), **Audience** (words sized to fill,
  section-tinted glow, title card, dark in instrumentals; layer
  Section/Slides/Lines). The view picker is a **dropdown that appears only
  while the mouse moves** over the panel (2.5 s linger). The choice is held
  across songs (`LyricsChoice` context in the desktop shell). The panel
  runs **ahead** of the song: sections/slides 1 s early (a pickup brings
  its section in), lines 0.4 s early.

### The Overview and the app

- DAW tab = the arrangement only. The chart editor is a column in the
  Overview beside the chart, toggled by an **Edit** button at the chart's
  top-left; Organize opens it, other modes close it.
- Overview left column = one pane: the chart (full width, `aspect-ratio`
  of a fitted page + 30% of the next) over the lyrics.
- The chart's paged fit shows the page plus the next page's first measure.
- Organize toolbar: one row, scrolls sideways.
- Mixer open/closed remembered per mode and view (`MixerMemory`); Organize
  starts closed.
- A song opens fitted: `z v` then `z x` on first paint.

## 4. The user's preferences learned this session

- Wants fixes in the tool, not workarounds in data ("we should be fixing
  the keyflow parser itself").
- Keyflow folder tracks stay hidden except Lyrics; chords live on the ruler.
- The chart preview takes the full width; no gaps between panes.
- Lyrics switch a little early; section changes must be obvious on the
  stage display (colour + rail).
- Delegates: happy for subagents to take keyflow parser work.

## 5. Known issues and open threads

- **The lyrics panel reads the Lyrics track once**, when the song mounts;
  moving lyric items in the arrangement does not update it until the song
  is reopened. Next obvious fix (re-read on `studio::request_resync`).
- Word / Syllable / Syllable+melody layers have types, no workflow (the
  user: "later"). keyflow-sync has an alignment pipeline to lean on.
- Spotify/Deezer/Musixmatch providers not written (Spotify needs the
  user's own token via env var — never handle secrets).
- Lyrics are not in the web demo.
- keyflow: `050_chart_view::nashville_and_roman_end_to_end` fails (expects
  `6m`, gets `6`) — pre-existing, not ours.
- session: `golden_scenes` — 8 mixer scenes drifted before this work.
- `~/.config/fts/guide-samples/Counts/English Female - 8.wav` missing
  (every prepare warns).
- `daw/docs/guides/keyflow/*.md` is a drifted copy of keyflow's guides;
  only keyflow's has the new sections/chords wording.
- From the previous handoff, still open: DAW → `.kf` two-way sync;
  left/right rail toolbars (ask what the user means); a section/measure
  progress bar under the main one; seamless song switching (device gap);
  the web demo (setlist, peaks); tab titles clipped mid-glyph;
  `chart_to_layout` counts bpm in quarter notes; worktree
  `/Volumes/build-disk/development/session-onepath` can be removed.

## 6. What is next

In the user's order of interest: keep polishing the lyric views (they
react fast — screenshot every change); lyrics word/syllable workflows;
DAW → `.kf` sync; the web demo (then replace `/demo` in `apps/web`,
deploy only after the user approves); the iPhone app.

## 7. Traps learned the hard way

- **Blitz: absolutely placed content inside a flex item that gets its
  size from the row's stretch is laid out against the PRE-stretch size**
  (2 px): the chart editor was a one-line sliver until clicked. Give the
  column an explicit `height:100%`. Headless probes don't show it.
- **Blitz: `aspect-ratio` + `max-height`** shrinks the WIDTH to keep the
  ratio — the chart went narrow. Use one or the other.
- Blitz does support `transform: rotate(…)` (the rails) and
  `radial-gradient`.
- **Blitz crash "invalid key" in `snapshot_node`** — a click on a node the
  click removes (a closing menu). Fixed in the blitz branch (`e30bcb29`,
  with a test); if another "invalid key" shows, look for the same shape.
- **rsx format strings can't hold an inline `if`** (`{if a {1} else {2}}`)
  — compute a variable first.
- Item labels only persist because of the `<NOTES>` fix — anything that
  names items (chords, keys, lyric lines) depends on it.
- `prepare` rebuilds from the **`.RPP`**, not the `.session`: re-preparing
  discards anything edited only in the session (e.g. lyric items moved by
  hand). Once hand edits matter, prepare must stop being the way to apply
  a `.lrc`.
- dynamic-template has a deliberate **track-count tripwire**
  (`golden_session::rpp` asserts 278 since Lyrics was added).
- **Testing keyflow alone doesn't resolve** (its lockfile wants tags that
  don't exist: `keyflow-proto` from daw's local branch, architect
  `v0.7.5`). Borrow the session lockfile and patch both crates, restoring
  keyflow's own `Cargo.lock` after:

  ```bash
  cd /Volumes/build-disk/development/keyflow
  cp Cargo.lock /tmp/kf.lock && cp ../session/Cargo.lock Cargo.lock
  P='patch."https://github.com/FastTrackStudios/daw"'
  cargo test -p keyflow-text --config "$P.keyflow-proto.path=\"../daw/crates/keyflow/keyflow-proto\"" \
                             --config "$P.keyflow-syntax.path=\"../daw/crates/keyflow/keyflow-syntax\""
  cp /tmp/kf.lock Cargo.lock
  ```

  keyflow's and session's `Cargo.lock` carry uncommitted changes that are
  not ours — never commit them.
- Probing a chart quickly: a throwaway test in
  `features/engraver/proto/src/engraver/layout/chart/tests.rs` that
  `keyflow_text::chart::parse_chart`s a file and prints each bar's
  `full_symbol@position.total_duration.beat` (and `push_pull`), run with
  the recipe above, then the file restored. `prepare` also refuses a chart
  whose bar counts don't add up.
- From before: never run cargo inside `nix develop`; a `.peek()` in an
  `if let` scrutinee holds its borrow; `git add justfile` matches nothing
  (it is `Justfile`); guide generation writes one item per role — don't
  clear past the song; `!T2/4` on a section *header* line is not a meter
  change; the in-app browser pane has no audio; the box's load is often
  50–150 — compare CPU time, not wall time.

## 8. Repos, branches, and what must be pushed first

| repo | branch | local-only work |
|---|---|---|
| `session` | `macos-compat` | ~80 commits past `origin/main` (today: the lyrics system, Overview, editor toggle, mixer memory, fit-on-open, signing, chord track) |
| `daw` | `tag-section` | ~29: `.session` bridge, sessionpeaks, anchor_media, marker GUID; today: add2/add4, chord-memory setting, **item labels through `.session`** |
| `keyflow` | `tag-section` | today: cursor meter fix, chord memory off, 42=add2, parser fixes (parens, bare r, bare PRE/Post, counted PRE/Post first), LotF tests ignored |
| `editor` | `main` (3 ahead of origin) | test CSS path, typst/mermaid as features, `EDITOR_CSS` |
| `blitz` | `session-stale-mousedown` | the drag panic fix **and the focus "invalid key" fix** |
| `task` | `session-share-cli` | share links (untouched since the 22nd) |

Session's `Cargo.toml` `[patch]` tables point at sibling paths for `daw`
(keyflow-proto, keyflow-syntax, daw crates), `keyflow` (engraver-proto,
editor-keyflow-lang, keyflow, keyflow-text), `editor` (editor-view, -state,
-vim, -syntax) and `blitz-*`. Before session is pushed: push those branches
or tags, move the git pins (the blitz `rev`, the keyflow and editor tags,
daw's tag) and drop the path entries. Data outside git: the songs' `.kf`,
`.lrc` and `.session` files under `/Volumes/build-disk/development/sessions/`.

## 9. Files worth reading first

| what | where |
|---|---|
| lyrics model, anchors, stamping | `crates/session/session/src/lyrics.rs` |
| lyrics panel (3 views, dropdown) | `apps/session-daw/src/lyrics_panel.rs` |
| lyrics fetcher (LRCLIB) | `apps/cli/src/lyrics/` |
| prepare, stamp_lyrics, apply_chart | `apps/session-daw/src/prepare.rs` |
| the Overview layout | `apps/session-daw/src/shell.rs` (`OverviewLayout`) |
| the desktop shell (views, editor toggle, contexts) | `apps/desktop/src/native/shell.rs` |
| chart panel (paged fit, next-page peek) | `apps/session-daw/src/chart_panel.rs` |
| chord track voicings (pushes, rests) | `crates/session/session/src/keyflow/generate.rs` |
| rebuild_from_chart | `crates/session/session/src/keyflow/from_chart.rs` |
| mixer memory | `apps/session-daw/src/mixer_panel.rs` |
| `.session` save/load (labels) | `daw/features/standalone/daw-standalone/src/session_file.rs`, `project_loader.rs` |
| chord parsing (add2/add4, parens) | `daw/crates/keyflow/keyflow-proto/src/chord/definition.rs`, `normalization.rs` |
| chord memory setting | `daw/crates/keyflow/keyflow-proto/src/chart/memory.rs`, `settings.rs` |
| chart parser | `keyflow/crates/keyflow/keyflow-text/src/chart/parser/{chords,sections,metadata}.rs` |
| open, setlist, tabs | `apps/session-daw/src/open.rs`, `setlist.rs`, `shell.rs` |

## 10. The browser demo and Task (unchanged since 2026-09-22 morning)

```bash
# Task's local servers (ACME on :18080) — plants the demo world first.
cd task && just demo serve           # background it; logs to the scratchpad

# The CLI, signed in as a demo owner (its own session dir keeps your real one clean)
export XDG_DATA_HOME=/tmp/task-cli TASK_PASSWORD=correct-horse-battery-staple
task/target/debug/task auth login --server ws://127.0.0.1:18080/vox \
    --org acme-audio --email alice@acme.test

# A session into Task, and a public link to it
task files root ensure session/<slug> --name "<Song>"
task files put <root-id> "<session>/<Song>.RPP"
task files put <root-id> "<session>/Media" --to Media
task files checkpoint <root-id> --message "…"
task share folder <root-id> --label demo --documents

# Proxies and the guide library (Ogg)
cd session
cargo run -p session-cli -- proxies "../sessions/imported/<Song>"
cargo run -p session-cli -- guide-library ~/.config/fts/guide-samples /tmp/guide-ogg

# The web bundle (release + wasm-opt, ~2 min), and a static server on :8765
just web-daw                      # optionally: SESSION RPP CHART to bake a session in
just web-daw-serve
```

The demo page takes its session from the query:
`http://localhost:8765/?share=<link>&project=<Song>.RPP&chart=<Song>.kf&guide=<guide link>`.
Re-mint the share links after a fresh `just demo fresh`.
