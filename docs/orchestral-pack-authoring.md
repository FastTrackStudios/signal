# Authoring an orchestral soundpack (styx only, zero code)

How to make the sampler engine play a new orchestral library — CSS Celli,
CSW flute, a brass section, a harp — by writing **data only**. The engine
(`signal-sampler`) is the interpreter: it implements the *mechanisms* (zone
resolution, mono-line legato scheduling, crossfading, envelopes, keyswitch
dispatch); the pack carries the *policy* — every number and every behavioral
selection. Nothing library-specific is compiled into the engine; the grep
test is `grep -ri css features/sampler/signal-sampler/src/engine/` → nothing
but generic mechanism.

The worked example throughout is Cinematic Studio Strings — the reference
pack lives at `features/rigs/orchestra/specs/cinematic-strings.styx` and is
parity-tested against a real CSS-in-Kontakt render (`tests/ab_pernote.py`,
`tests/legato_lookahead.rs`).

## 1. A pack is inventory + interpretation — two separable layers

r[signal.soundsource.declarative]

| layer | what | file | how it's made |
|---|---|---|---|
| **sample inventory** | zones: file × key range × velocity range × RR × mic × articulation tag × dynamic tag (+ measured transition metadata) | `<library>/_patches/<Section>/library.styx` (or the styx embedded in a `.signalpack`) | *generated* by the extractor/scanner |
| **interpretation** | how the inventory is *played*: articulation semantics, keyswitch map, dynamics mapping, legato timing model, envelopes, makeup/tune | `cinematic-strings.styx` (the authored config) | *authored* — this document |

`PlayerPatch::load_merged(config, zones, samples_root)` overlays the
authored interpretation onto the generated inventory (`sections`, `mics`,
`dynamics`, `articulations`, `legato_engine`, `short_note_timing`,
`keyswitch`, `performance` come from the config; `zones` stay from the
inventory). The interpretation layer is therefore the pack's **built-in
default performance implementation** — structurally separable, so a rig
preset or user can later override parts of it without touching (forking)
the sample inventory.

Every interpretation field has a default equal to the engine's historical
behavior — an inventory with *no* config at all still plays.

## 2. The interpretation schema, section by section

### 2.1 Identity + sections + mics

```styx
name    "Cinematic Strings"
vendor  "Cinematic Studio"
sections ( {id 1v, label "1st Violins", note_grid (G A B C# D# F),
            lowest_note G2, highest_note C#6} … )
mics ( {id Mix, label Mix, kind blended, default true}
       {id Main, label Main, kind separate} … )
```

One config file covers all sections of a library (Violin 1 / Celli / …);
the per-section inventory chooses which zones exist. `note_grid` documents
the sampling grid; `performance.zone_pitch_tolerance` (below) says how far
the engine may pitch-shift to fill the gaps.

### 2.2 `performance {}` — library-wide playback numbers

```styx
performance {
    sustain_noteoff_ms   400   // key-up overlap fade for held sustains
    sustain_makeup_db    6.0   // looped-plateau → vendor-render level offset
    master_tune_cents    9.0   // global tune (CSS ships +9 cents; default 0)
    loop_xfade_ms        150   // synth-loop seam crossfade
    zone_pitch_tolerance 2     // max semitones of grid-fill pitch shift
    release_gain         1.0   // recorded release-tail gain
    attack_ms            198   // default sustain attack bloom at load
    release_ms           400   // default note-off release at load
}
```

All optional; defaults are the engine's previous constants. A new library
usually only needs `master_tune_cents` (if the vendor detunes globally) and
`attack_ms`/`release_ms` (its "arco attack"/note-off feel).

### 2.3 `dynamics {}` — how loudness/timbre is controlled

```styx
dynamics {
    sustain_controller CC1          // omit for velocity-only libraries (harp)
    vibrato_controller CC2          // omit when the library has no vibrato control
    vibrato_mode       crossfade    // "crossfade" (CSS) | "on_off" (CSW switch)
    short_note_controller velocity
    cc1_layers_3 ( {label p, cc_range (0 55)} {label mf, cc_range (30 100)}
                   {label ff, cc_range (77 127)} )
    …cc1_layers_2/4/5/6 for other layer counts…
    short_note_cc1_map { 0-31 Spiccato, 32-63 Staccatissimo, … }
    cc1_expression { knee 45, floor_db -5.4 }   // bottom rolloff on top of the xfade
    enable_velocity_layers true                  // per-artic vel_thresholds bands
}
```

This expresses the keyflow-orchestra profile variation directly:
- CSS strings: `vibrato_controller CC2` + `vibrato_mode crossfade`.
- CSW winds: `vibrato_mode on_off`.
- Brass: omit `vibrato_controller`.
- Harp/generic: omit `sustain_controller` (dynamics ride velocity); the
  articulations then use `dyn_ctrl velocity`.

### 2.4 `articulations ( … )` — semantics per playing technique

Each articulation names its zones tag and describes behavior:

```styx
{
    id            Nonvib          // must match the inventory's zone tag
    label         "Non-Vibrato Sustain"
    kind          @Sustain        // @Sustain @Short @Legato @Release @Trill @OneShot @Looped @Special
    dynamics      (p mf ff)       // recorded layers, soft → loud
    rr            1
    dyn_ctrl      cc1             // cc1 | velocity | fixed
    release_artic NVrel           // recorded release tail to fire on key-up
    vibrato       false           // which side of the CC2 crossfade
    vibrato_pair  Vibsus          // its CC2 counterpart ("" = explicitly none)
    amp_env ( {time_ms 4.0, level 1.0, curve 0.505} … )   // decoded amp envelope
    amp_env_hold  true            // freeze at hold level while the key is down
}
```

**Transition selection is data.** For `kind @Legato` articulations:

- `legato_role` — `transition` (interval move), `retrigger` (same-note
  re-bow), `portamento` (glide).
- `vibrato` / `vibrato_pair` — the CC2 pairing of transition sets.
- `sordino` / `sordino_pair` — the muted-variant mapping.

When these are omitted the engine falls back to the CSS naming conventions
(`NV`/`Nonvib` = non-vibrato side, `zero` = retrigger, `port` = portamento,
`Sord` prefix = sordino) — implemented in the *spec layer*
(`spec.rs: ArticulationSpec::{is_vibrato, resolve_legato_role, is_sordino}`),
never in the engine. A library with different naming just sets the fields.

`amp_env` overrides the built-in family defaults
(`spec::default_amp_env` — the decoded CSS ENV_FLEX tables, kept as
defaults so packs without envelopes play unchanged). `vel_thresholds` /
`vel_layer_db` express per-articulation short-note velocity→layer bands.

### 2.5 `legato_engine {}` — the timing/crossfade model

r[signal.soundsource.legato] r[signal.soundsource.legato.velocity-zones]

```styx
legato_engine {
    // Velocity → pre-delay zones, per mode. A library WITHOUT a mode
    // toggle (CS Brass) writes flat `zones (…)` instead of the two modes.
    expressive  { enabled_cc58_range 6-10
                  zones ( {vel_range (0 64),   label slow,   delay_ms 333}
                          {vel_range (65 100), label medium, delay_ms 250}
                          {vel_range (101 127),label fast,   delay_ms 100} ) }
    low_latency { enabled_cc58_range 0-5
                  zones ( {vel_range (0 64),  label medium, delay_ms 150}
                          {vel_range (65 127),label fast,   delay_ms 100} ) }

    portamento { trigger_vel_max 20, volume_controller CC5 }
    retrigger  { trigger sustain_pedal_held, rr 3 }

    velocity_splits (64 100)          // attack-velocity ranges for the curves below
    overlap_delay { low_latency { soft {thresholds_ms (…), anchors_ms (…)}
                                  loud {…} }
                    expressive  { … } }      // live wait before the transition fires
    start_offset { thresholds_ms (100 150 500), anchors_ms (177 177 117) }
                                       // how deep into the transition recording playback starts
    transition_fade_ms   30
    retire_transition_ms (150 281 281) // previous-pair fades, by velocity range
    retire_sustain_ms    (550 500 500)
    sustain_trim_db      -6.0          // connected note sits under a fresh attack
    fallback_velocity    80
    skip_declick_ms      25
}
```

Examples across libraries (values from the vendors' manuals):
- **CSW flute**: same shape, `expressive 220/130/90`, `low_latency 90/70`.
- **CS Brass trombone/horn**: no mode toggle — flat
  `zones ( {vel_range (0 100), delay_ms 230} {vel_range (101 127), delay_ms 100} )`.
- **Harp**: omit `legato_engine` entirely → polyphonic, no mono line, no
  transitions.

The `overlap_delay`/`start_offset` curves and the retire fades only matter
for libraries whose transitions are *recorded* samples; omit them and the
decoded-CSS defaults apply (they are conservative and near-zero except
soft+fast playing).

**Live vs offline** (r[signal.soundsource.mode.parity]): all of the above
is one configuration. Live play uses the low-latency tables reactively
(bounded latency, no lookahead); document/offline rendering pre-rolls each
transition so its *arrival lands exactly on the tick*, using the measured
per-zone `arrival_ms` (falling back to `lead_in_ms`) from the inventory
capped by the mode's velocity-zone delay. Same samples, same spec — only
scheduling differs.

### 2.5b Grid placement policies (r[signal.sampling.markers.arrival])

Two placement policies govern where a note's audio sits against the grid
tick in document scheduling; which applies is a property of the note's
ROLE, plus one authored switch:

* **arrive-at-tick** — the trigger pre-rolls so the zone's measured
  heard-arrival (`arrival_ms`) lands exactly ON the tick; audio from the
  recording plays before the click. Always used for:
  - **legato transitions and re-bows** — the pre-click content is the
    PREVIOUS note continuing (the recorded bow change), which is correct
    musical behaviour;
  - **shorts** — the pre-click content is the recorded attack noise before
    the rhythmic peak (the per-RR replacement for the single global
    `short_note_timing.pre_delay_ms`).
* **start-at-tick** — the sample STARTS on the tick and speaks naturally
  after it: nothing sounds before the click the note begins on. Authored
  per library for FRESH sustain attacks (phrase starts) via
  `performance { attack_placement start_at_tick }`; the default
  (`arrive_at_tick` / unset) pre-rolls fresh sustains by their measured
  perceptual-onset bound instead. CSS Violin 1 ships `start_at_tick` —
  it matches what the vendor instrument does live, and the owner's ear:
  no audio before the beat a note starts on.

### 2.6 `keyswitch {}` — articulation selection sources

```styx
keyswitch {
    velocity_sensitive true
    cc58_map { 0-5 "Sustain: Low Latency Legato", 21-25 Staccato, … }
    notes ( { note "C0", label "Sustain",
              vel_map { 0-64 "Nonvib+@legato-low", 65-127 "Nonvib+@legato-expressive" } } … )
}
```

Selection sources are all data: CC ranges (`cc58_map` — any layout),
velocity-sensitive keyswitch notes (`notes`), or **neither** — omit
`keyswitch` entirely for a no-keyswitch library (generic/harp profile);
the engine then plays the default articulation, and a rig can still pin
one programmatically (`pin_articulation`).

`@`-tokens are named mode switches the interpreter implements:
`@legato-on/off`, `@legato-expressive/low`, `@sordino-on/off`, `@novib`.
`+` combines a zone selection with mode switches.

**Latched-CC selector (UACC).** For libraries that follow Spitfire's UACC
convention — one latched CC whose *value* is a standardized articulation
code — enable the selector at the top level of the spec:

```styx
selector    uacc      // latched-CC articulation selector, UACC defaults (CC32)
selector_cc 32        // optional — omit for the UACC default CC32
```

`selector uacc` alone gives conventionally-named articulations the
published standard codes (`Long/Sustain` 1, `Legato` 20, `Staccato` 40,
`Spiccato` 42, `Pizzicato` 56, `Col Legno` 58, `Tremolo` 11, `Harmonics`
10, trills 70+, … — the core of the Spitfire UACC v2 table, shipped as
data in `spec::UACC_STANDARD_TABLE`). Any articulation can override or
opt in explicitly with a per-articulation code, which always wins:

```styx
articulations (
    { id Shorts2, label "Alt Staccato", kind @Short, uacc 41 }
)
```

Mechanically it is a **latched CC**: a value on the selector CC before a
note-on latches that articulation for all subsequent notes (exactly like
a keyswitch latch) — live and in offline document renders (the scheduler
forwards selector CC events and derives short pre-roll / legato prefire
timing from the selected articulation's `kind`). Codes with no matching
articulation leave the previous latch untouched. `keyswitch {}` and
`selector` can coexist; omit both for a no-keyswitch library.

### 2.7 `short_note_timing {}`

```styx
short_note_timing { pre_delay_ms 60 }   // recorded pre-roll before the rhythmic peak
```

Offline rendering pre-rolls shorts by this; live play fires immediately.

## 3. Named strategy points (enums, not hardcodes)

Behaviors that are *selected* by data and *implemented* by the interpreter.
If a new library needs a behavior none of these express, add a **new named
variant** here (a strategy the engine implements generically) — never a
library check in engine code:

| field | variants |
|---|---|
| `articulations[].kind` | `@Sustain @Short @Legato @Release @Trill @OneShot @Looped @Special` |
| `articulations[].dyn_ctrl` | `cc1`, `velocity`, `fixed` |
| `articulations[].legato_role` | `transition`, `retrigger`, `portamento` (default: infer from id) |
| `dynamics.vibrato_mode` | `crossfade`, `on_off` |
| `legato_engine.retrigger.trigger` | `sustain_pedal_held` (re-bow gate) |
| `zones[].rr_mode` | `cycle`, `random`, `no-repeat-random` |
| `zones[].trigger_mode` | attack, `one-shot`, `release`, `pedal-down/up`, `cc`, `aftertouch` |
| `zones[].playback_mode` | forward, `reverse`, `alternate` |
| keyswitch `@`-tokens | `@legato-*`, `@sordino-*`, `@novib` |
| `selector` | `uacc` (latched-CC selector, CC32 + published code table) |

## 4. Recipe: the next instrument

1. **Extract/scan the inventory** → per-section `library.styx` (or
   `.signalpack`). For legato libraries the generator also measures each
   transition's `lead_in_ms` + `interval` + `direction` — that metadata is
   what makes offline arrival-on-tick scheduling exact.
2. **Copy the closest config** (`cinematic-strings.styx` for a CSS-family
   library) and edit: sections, mics, articulation list + zone tags,
   dynamics controllers/mode, legato tables from the manual, keyswitch
   map, `performance` numbers.
3. **Validate**: `LibrarySpec::from_file` round-trip is covered by
   `cargo test -p signal-sampler --lib spec` (add a sibling test for the
   new file), and `styx check` validates syntax.
4. **Load it** exactly like CSS:
   `rig.load_instrument_with_config(id, config, zones, root, section, mic)`
   (see `signal_orchestra::load_strings` — nothing in that function is
   library-specific anymore).
5. **Verify behavior** with the generic harness: schedule-level tests
   (prefire leads follow YOUR tables — see
   `document.rs::prefire_lead_follows_spec_overlap_delay_curves`) and the
   audio A/B (`gen_css_ab`-style corpus + `ab_pernote.py`) if you have a
   vendor reference render.

## 5. What the engine still owns (mechanism, intentionally)

- Mono-line legato state machines, voice pools, crossfade laws,
  declick ramps (`SUSTAIN_DECLICK_MS`/`ONSET_DECLICK_MS`), loop-plateau
  detection, deterministic RR hashing, the document scheduler.
- The *shape* of every curve interpreter (`IoiCurveSpec::value_at`,
  `Cc1Layer` blending, `FlexEnv`).
- Safety bounds (voice lifetime caps, miss telemetry).

These have no per-library values; anything with a per-library value lives
in the pack.
