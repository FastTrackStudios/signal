# CSS A/B — matching our engine to a real Kontakt render

The measurement loop behind the Cinematic Studio Strings legato work. Rounds
1–7 (July 2026) ran this out of a scratch directory in a throwaway worktree;
when that worktree was deleted, the reference renders, the diff script and the
A/B page went with it, and every pack-side calibration on disk reverted to its
pre-round values. **That is why this lives in git now.** The only thing that
cannot live here is the reference audio itself (320 MB of Kontakt bounces).

## The loop

```
just css-ab                    # render ours, score every section, write ab.html
just css-ab-sections S10,S13   # same, one or two sections while iterating
```

or directly, which is what the recipe runs:

```
python3 features/sampler/signal-sampler/tests/css-ab/score.py \
    --pack "$LIB/1st Violins/Legato/1st Violins - Legato - Mix.signalpack"
```

It renders `CSS-Param-Test.mid` — the same MIDI the Kontakt reference was
bounced from — through `fts signal pack render-report --midi` with
`SIGNAL_NO_PREFIRE=1`, slices both renders into the manifest's 15 parameter
sections, and prints a scoreboard. Outputs land in `scratch/css-ab/`:
`ours.wav`, `ours.html` (our render's own trace report), and `ab.html`.

Needs `cargo build -p fts-cli --bin fts` first.

## Reading the scoreboard

Two numbers per section, deliberately separate:

- **lvl** — level ratio ref/ours. Pure loudness; a trim fixes it. 1.00 is dead
  on, and the band that counts as fine is 0.8–1.25.
- **shape** — mean |envelope difference| in dB *after* the level ratio is
  matched out. This is contour: attack shapes, swells, retire fades, release
  tails. It is what a fix to the legato *model* moves. It never reaches zero —
  round-robin variance means we and Kontakt play different takes of the same
  note, which floors the metric around 3 dB.

`raw` is the un-normalised mean, the number rounds 1–7 were scored on. It is
kept so old scoreboards stay comparable, but it mixes the two failure modes
together and a section can look bad purely because of a trim.

## The reference audio

Lives outside git under `$CSS_REF_DIR` (default
`/run/media/AudioHaven/Signal/Reference/CSS`), pulled off voyager (the Mac that
runs Kontakt):

| file | what |
|---|---|
| `CSS Param Test.wav` + `CSS-Param-Test.mid` | 124 s, 15 parameter sections — the main corpus (`../../scripts/css-param-test.manifest.json` describes it; `../../scripts/gen_css_test_midi.py` generates the MIDI) |
| `CSS C_Major Scale.wav` + `CSS-C-Major-Midi.mid` | the original one-note-per-beat scale, vel=1 CC1=80 |
| `css test ab render.wav` + `css_ab.mid` | the older 63-section corpus (calibration, dynamics, shorts, legato velocity + interval sweeps) |
| `CSS Test Export*.wav` | earliest bounces, superseded |

To re-bounce or extend the corpus: generate the MIDI with
`gen_css_test_midi.py`, render it in Kontakt on voyager against CSS 1st
Violins with the **Mix** mic only, bounce to 24-bit/48 kHz, and copy it back
(`ssh voyager "cd ~/Downloads && tar cf - '<file>'" | tar xf -` — voyager's
bundled rsync is too old for `--protect-args`, and the filenames have spaces).

## Calibration lives in the styx, and must be pushed into packs

Pack-side calibration (`sustain_makeup_db`, `release_ms`,
`cc1_expression.floor_db`) is authored in
`features/rigs/orchestra/specs/cinematic-strings.styx` — that file is the
source of truth. But every `.signalpack` embeds a *copy* of the spec taken at
build time, and the engine reads the embedded copy. A pack built before a
calibration change silently runs the old numbers.

Check, and re-inject if they drift:

```
fts signal pack spec get "<pack>" | grep -E 'sustain_makeup_db|release_ms |floor_db'
fts signal pack spec get "<pack>" > /tmp/pack.styx
# edit the drifted scalars to match cinematic-strings.styx
fts signal pack spec set "<pack>" /tmp/pack.styx
```

Edit the dumped text and set it back — never rebuild the whole embedded spec
from `cinematic-strings.styx`, which carries no zones and would drop the
measured per-zone data (`arrival_ms`, `lead_in`, loops) the packs hold.

## Where the model comes from

`../../docs/css-ksp-legato-algorithm.md` is the decode of CSS's KSP — §9 is
what an engine must replicate, §11 is the voice-lifecycle round-2 decode and
§11.6 its seven required engine changes. Fixes should cite it; when the decode
and the measurement disagree, say so in the commit rather than quietly tuning
a number until the scoreboard moves.
