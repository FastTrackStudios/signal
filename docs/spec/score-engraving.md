# Score Engraving — full orchestral scores + extracted parts

Status: draft (2026-07-28). Owner: engraver domain.

Goal: render **full orchestral scores** and **individual instrument parts**
from MusicXML through the existing engraver pipeline — the same engine that
lays out keyflow charts today. Target quality bar: the professionally
engraved Columbus Symphony Alan Parsons books
(`~/Downloads/Colombus Symphony Parsons Orchestral Parts/` — per-instrument
PDFs for 16 songs, including tacet sheets and multirest-heavy layouts).

## Practice corpus

| Source | What it is |
| --- | --- |
| `~/.fts-scratch/aplp-sample/games.musicxml` | Games People Play, real 23-part orchestral score (winds, brass, perc, strings, rhythm/vox) |
| `crates/keyflow/examples/png-project-charts/*.musicxml` | Tom Brooks Finale exports (lead-sheet-grade, harmony + melody + directions) |
| Columbus PDFs | Reference renderings only (visual ground truth, not input) |

`keyflow_orchestra::score::load` already parses games.musicxml (23 parts,
QN-domain playback model), so the file is known-good MusicXML 4.0.

## What we already have

The engraver was built as a Rust port of MuseScore's engraving stack, and
the **horizontal** half of that port is done and battle-tested by chart
layout:

- `engraver/layout/tlayout/*` — element layouts (note, rest, beam, clef,
  keysig, timesig, barline, tuplet, slur/tie, accidentals, dynamics,
  brackets, system dividers…), explicitly replacing MuseScore's
  `TLayout.cpp` with trait dispatch.
- `engraver/layout/{segment,segment_list,springs,spacing,skyline,shape}` —
  MuseScore's Segment/Shape/Skyline horizontal-spacing model (springs =
  MS 4.x justification).
- `engraver/notation/builder` — ChordRest-style rhythm entries, beam
  grouping, tuplet detection, quantize.
- Fonts (Bravura/Leland + SMuFL metadata), SVG + PDF export, multi-page
  chart orchestration (`layout/orchestrator.rs`).
- `keyflow-musicxml` — MusicXML → `Chart` (harmony, directions, voltas,
  dynamics, staff text, measure widths); single-staff mindset.
- `keyflow-orchestra/score` — MusicXML → playback `Score` (MIDI pitch,
  QN onsets, articulation tags). **Not** an engraving model: written
  spelling, rests, clefs, and voice structure are discarded.

What was deliberately removed: the old MuseScore-style
`Score/Part/Voice/Measure` model in `engraver/model/` (superseded by the
chart-centric pipeline; only leaf types survive). Score engraving brings a
model like it back — but as a thin notation IR, not a full editor DOM.

## The gap

1. **Notation score model** — written pitch (step/alter/octave), voices,
   rests, clef/key/time changes, per-part staves (piano = 2), ties/slurs as
   spanners, directions. MuseScore analogue: `Score → Part → Staff →
   Measure → Segment → ChordRest`. Ours can be immutable and layout-only.
2. **MusicXML → model importer** preserving spelling. Reuse the `musicxml`
   crate dependency from `keyflow-musicxml`; the measure-walk skeleton in
   `keyflow-orchestra/score/parse.rs` shows the traversal.
3. **Vertical/system layout** — the un-ported half of MuseScore:
   - system-level segment alignment (one x-grid shared by all staves in a
     system; MuseScore does this by laying out Segments per-system across
     staves),
   - staff distance via skylines (`SysStaff` spacing),
   - bracket/brace columns, barlines spanning staff groups,
   - system/page breaking for N-staff systems (chart orchestrator only
     breaks single-staff systems today).
4. **Part extraction** — MuseScore models parts as linked Excerpts; we can
   re-run layout over a single-part filter of the same model instead.
   Needs: multi-measure rests (the Columbus parts are full of them), tacet
   sheets, transposed display (B♭/F parts), cue-size notes later.

## Plan

- **P1 — model + import.** `engraver-score` module (or crate under
  `features/engraver/`): notation IR + MusicXML importer. Acceptance:
  round-trip inventory of games.musicxml (23 parts, note counts, voices,
  clefs, key/time changes) matches the file.
- **P2 — single-part rendering.** One staff, systems + pages via the
  existing orchestrator; beams/tuplets/accidentals from tlayout. Render
  Violin from games.musicxml to PDF next to the Columbus `GAMES PEOPLE
  PLAY - Violin 1.pdf` and eyeball. Multirests land here (parts are
  unreadable without them).
- **P3 — multi-staff systems.** Shared segment x-grid across staves, staff
  distance skylines, brackets/braces, group barlines. Render the full
  games.musicxml score (23 staves, likely landscape/rastral scaling).
- **P4 — parts product.** Part extraction over the score model: title
  block/instrument header, tacet sheets, transposition display. Batch:
  all parts of a song → per-instrument PDFs (the Columbus book,
  regenerated).
- **P5 — polish.** Dynamics/hairpins/slur quality, rehearsal marks at
  system starts, text styles, cue notes.

MuseScore references (MIT, mu4): `src/engraving/rendering/score/` —
`systemlayout.cpp` (system assembly + staff distances), `pagelayout.cpp`,
`measurelayout.cpp`, `horizontalspacing.cpp` (already ported in spirit),
`skyline.cpp` (`infrastructure`). Read for the vertical pass; do not port
the editor DOM.

## Relation to keyflow

Charts stay the product for the band; score engraving serves orchestral
books (Alan Parsons tour, CBU-style concerts) and, later, printed melody
lines inside charts. `keyflow-musicxml` (Chart import) and `engraver-score`
(notation import) stay separate consumers of the same `musicxml` parse.
