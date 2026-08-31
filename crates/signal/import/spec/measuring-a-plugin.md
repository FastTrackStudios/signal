# Measuring a plugin you intend to replace

How to take a third-party plugin apart well enough to convert its presets
into an FTS plugin, and how to prove the result sounds the same. Written
after doing it for Pro-Q 4 and Pro-C 3, and specifically so it can be done
again on a machine that has a plugin this one does not — Pro-C 2 being the
case in hand.

Companion to [`project-state-formats.md`](project-state-formats.md), which
records what the formats turned out to *be*. This one records how to find
out, and what goes wrong.

---

## 0. What the machine needs

- The plugin **installed and authorised**. Not negotiable and not
  work-around-able: every unit in §2 comes from asking the plugin, and an
  unauthorised plugin either refuses to load or renders silence. This is
  usually the only reason the work has to move machines at all.
- Its **factory presets** on disk (`~/Documents/FabFilter/Presets/<name>/`).
- This repo, and `nix develop` for the toolchain.
- On Linux, FabFilter plugins are Windows binaries reached through
  yabridge — `~/.clap/yabridge/…` rather than `~/.clap/…`.

Check the plugin loads at all before anything else:

```sh
cargo run --release -p signal-plugin-host --example load_plugin -- \
    ~/.clap/yabridge/"FabFilter Pro-C 2.clap"
```

It prints the name, the CLAP id and the parameter count. Note the id — the
converter matches on it, and it is not always what you would guess (Pro-C 2
in a real project is `com.FabFilter.preset-discovery.Pro-C.2`, not
`com.FabFilter.Pro-C.2`).

---

## 1. Layout — which float is which

FabFilter plugins save a flat vector of `f32` behind an `FFBS` header. The
vector is unlabelled. The labels come from the preset files, free:

```sh
cargo run --release -p signal-import --example ffp_survey -- \
    ~/Documents/FabFilter/Presets/"Pro-C 2"
```

A `.ffp` text preset lists the same values, in the same order the plugin
publishes its parameters, under readable names. Confirm the three counts
agree — preset keys, `load_plugin`'s parameter count, and the `N` in the
`FFBS` header of a real instance — and the field table is done.

The survey prints two things worth reading closely.

**Layout variants.** A preset folder is not one format. Six of Pro-C 3's 122
factory presets carry 69 keys in a different order under the same `FC3p`
signature — an older layout the plugin still opens. This is why the rule is:

> Decode the **binary state** by index, because the installed build wrote it.
> Decode preset **files** by key name, because they did not.

Getting this backwards produces a decoder that works on most presets and
silently scrambles the rest.

**What is load-bearing.** A parameter constant across the whole factory
library can be carried as a default and forgotten. One with eighty distinct
values needs its units measured. The survey's `distinct` column sorts the
work for you.

---

## 2. Units — what a stored float means

This is where the real errors live. `Ratio=0.56` is not a ratio.
`Attack=0.0993` is not milliseconds. `Output Level=0.3` is not a third of a
decibel — it is **+10.8 dB**, and reading it as decibels was the single
largest error in the Pro-Q library, wrong on 68 of 171 presets.

So ask the plugin:

```sh
cargo run --release -p signal-analyzer --example plugin_params -- \
    --plugin ~/.clap/yabridge/"FabFilter Pro-C 2.clap" \
    > proc2-encodings.txt
```

It sweeps each parameter across its declared range and prints the plugin's
own display text at each point. Narrow with `--only ratio,attack`, and
control resolution with `--steps`.

### Two facts that make this cheap

**The stored float, the `.ffp` text value, and what a host's `set_param`
takes are all the same number.** Not normalised — plain. Confirm it once per
plugin by setting a parameter and reading the saved state back; it held for
Pro-Q 4 and Pro-C 3. Everything else in this section depends on it.

**`value_to_text` is a pure query.** No parameter set, no render, no state
save. An earlier version of this probe did set/render/save at every point and
took **over an hour** for nineteen parameters across the yabridge boundary,
which is not a tool anyone iterates with. The query version does the whole
hundred in a couple of minutes.

### Reading the output

Each parameter falls into one of four kinds.

| What you see | What it is | What to write |
|---|---|---|
| Display equals the stored value | plain units already | nothing |
| Display is a fixed multiple | linear scaling | a constant |
| Display repeats then jumps | an enum | a name table |
| Display curves | an encoding | a closed form, or a table |

The enums matter more than they look. Pro-C 3 has **fourteen** styles, and an
existing table in `parser.rs` guessed eight, with the wrong name from index 1
onward. The probe prints only the points where the text changes, so a
fourteen-way selector reads as fourteen lines.

### Encodings already established

These recur across FabFilter plugins and are worth trying first:

| Encoding | Form | Seen in |
|---|---|---|
| Frequency | `2^stored` Hz (log2) | Pro-Q 4 bands, Pro-C 3 side-chain EQ |
| Q | `0.025 · 1600^stored` | both, identically |
| Level faders | **36 dB per unit**, tapering to silence below `-0.6` | Pro-Q Output Level, all four Pro-C faders |
| Attack | `0.005 + 250·x³` ms | Pro-C 3 |

The fader is the one to check first on any new plugin, because it is silent
when wrong: the preset loads, the curve is right, and the level is out by
several dB.

---

## 3. From measurement to code

Prefer a closed form when the data supports one across the whole range — the
Pro-C attack law is exact against sixty sampled points. Do not force one when
it does not: Pro-C's release passes through 10 ms, 21.62, 56.50 and 2.5 s,
which no power law or exponential reaches, and a fitted curve that is wrong
in the middle is worse than sixty points that are right everywhere.

For those, emit a measured table straight from the plugin:

```sh
cargo run --release -p signal-analyzer --example plugin_params -- \
    --plugin ~/.clap/yabridge/"FabFilter Pro-C 2.clap" \
    --only release --rust release
```

That prints a `const …_CURVE: [(f64, f64); N]` ready to paste, with seconds
folded to milliseconds and any non-numeric entries listed as a comment rather
than dropped. Interpolate with the `read_curve` helper in
[`proc3.rs`](../src/fabfilter/proc3.rs).

Layout goes in `src/fabfilter/<plugin>.rs` as a `field` module of index
constants, a struct in real units, and a `decode(&FfbsState)`. Keep the
comments carrying the numbers they came from — a constant with no measurement
attached is one nobody can later check.

---

## 4. Parity — does the replacement sound the same

Translation being *plausible* is not the same as it being *right*. Three
tools, in increasing scope.

**Does our plugin pass audio at all**, with the state the converter wrote:

```sh
cargo run --release -p signal-analyzer --example comp_bundle_check -- \
    --state <state.json> --reference ~/.clap/yabridge/"FabFilter Pro-C 2.clap"
```

**Does one control behave**, ours against the engine:

```sh
cargo run --release -p signal-analyzer --example eq_bundle_check
```

**Does a whole project convert**, every instance measured against both real
plugins:

```sh
cargo build --release -p fts-convert
./target/release/fts-convert song.rpp --dry-run --engine-too
```

`--verify` is on by default; `--no-verify` turns it off. `--engine-too` adds
the engine's own error beside the plugin's, and `--curves <track name>`
prints the three response curves for one instance. Nothing is written without
`--in-place`; the default output is `<name>.fts.rpp` beside the original.

### The decision table

This is the part worth internalising. When the report shows a gap:

| engine column | plugin column | Where the fault is |
|---|---|---|
| small | small | nowhere — this instance is fine |
| small | large | **the parameter map** — naming or units in `rpp/fts_*.rs` |
| large | large | the DSP — a job for the engine work, not the converter |
| — | "not measurable" | one plugin rendered silence; see §5 |

Two columns is what makes this a diagnosis rather than a hypothesis. Without
it, the state-completeness bug in §5 read as a DSP problem and would have
been chased in the wrong crate.

The stimulus is broadband noise, which is the right question for an equalizer
and only half of it for a compressor: it says the two agree on how much they
pull down at each frequency, not that they agree on how they get there.
Reading a compressor properly wants programme material with a crest factor.
Treat the compressor numbers as a floor.

---

## 5. The traps, in the order they cost the most

Every one of these was found by measurement after passing review by eye.

**A plugin state sets what it names and resets nothing.** A converted
instance that writes only the parameters its preset uses inherits everything
else from whatever the plugin held before — so presets late in a chain came
back carrying the *previous* preset's dynamics, up to 6 dB out, and perfectly
plausible in isolation. Write every parameter, every time. It costs about
20 kB per FX block.

**Target ranges narrower than the source clamp silently.** Pro-C limits at
100:1 and FTS Comp stopped at 20:1, so every limiting preset arrived as a
20:1 compressor. Same for knee (72 vs 24), the trims (±36 vs ±24), and on the
equalizer the shape index (12 vs 9, so three filter types landed on All Pass)
and Q (40 vs 18). **Check every target range against the source's before
translating anything.**

**Dead lookup tables that assert a false invariant.** The EQ plugin carried a
`plugin_shape` table claiming its persisted order swapped Low Cut and High
Shelf. Nothing called it — the audio path passed the index straight through —
so believing it would have turned every high shelf into a high-pass. If a
table is not on the path, delete it rather than trusting it.

**Reusing one plugin instance across many `load_state` calls.** Correct, and
the reason the state-completeness bug was invisible until instance eight.
Keep doing it, but read the numbers in order and be suspicious of a gap that
grows down the list.

**The 36 dB fader.** See §2. Wrong on 68 of 171 Pro-Q presets.

---

## 6. Known blockers

**Pro-C renders silence through `signal-plugin-host`.** At its own default,
with no state loaded, where Pro-Q 4 through the identical path renders
correctly. `HostedPlugin::process_interleaved` gives a plugin one stereo
input and nothing else, and Pro-C declares a side-chain bus it never
receives; that is the likely cause and it is **not settled**. Fixing it means
aux-bus support in `daw::plugin::PluginInstance`, which lives in the
[daw](https://github.com/FastTrackStudios/daw) repo — a cross-repo change
needing a tag bump here.

Until then a compressor conversion can be *built* and unit-tested but not
*heard*, and `fts-convert` reports "not measurable (one side was silent)"
rather than an infinite error. Do not read that as a translation failure.

---

## 7. Runbook: Pro-C 2 on a machine that has it

Pro-C 2 is the compressor actually in use in these sessions:
`02 LORD OF THE FIGHT.RPP` carries **six** Pro-C 2 instances against three of
Pro-C 3 (and sixteen Pro-Q 4). It is not installed on this machine, so the
measurement has to happen where it is.

`fts-convert` already refuses Pro-C 2 rather than mistaking it for a 3 — the
version is part of the match, and its CLAP id is
`com.FabFilter.preset-discovery.Pro-C.2`, which is worth noticing before
writing a recogniser arm for it.

Everything below is read-only against the plugin. Nothing needs a project
file, and the artifacts are small text files to bring back.

```sh
# 0. confirm it loads, and note the CLAP id and parameter count
cargo run --release -p signal-plugin-host --example load_plugin -- \
    ~/.clap/yabridge/"FabFilter Pro-C 2.clap" | tee proc2-load.txt

# 1. layout: names, order, layout variants, what is load-bearing
cargo run --release -p signal-import --example ffp_survey -- \
    ~/Documents/FabFilter/Presets/"Pro-C 2" > proc2-survey.txt

# 2. units: every parameter, the plugin's own words
cargo run --release -p signal-analyzer --example plugin_params -- \
    --plugin ~/.clap/yabridge/"FabFilter Pro-C 2.clap" > proc2-encodings.txt

# 3. tables for anything that curves (repeat per parameter)
cargo run --release -p signal-analyzer --example plugin_params -- \
    --plugin ~/.clap/yabridge/"FabFilter Pro-C 2.clap" \
    --only release --rust release >> proc2-tables.rs
```

Bring back those four files. They are enough to write
`src/fabfilter/proc2.rs` and `src/rpp/fts_comp.rs`'s Pro-C 2 arm without the
plugin present — which is the whole point of doing it this way.

Two things to confirm while you are there, because they cannot be checked
from the artifacts:

1. **Does `set_param` take plain values here too?** Set one parameter, save
   the state, and check the stored float equals what you set. §2 depends on
   it.
2. **Does Pro-C 2 render silence through our host, as Pro-C 3 does?** Run
   `comp_bundle_check --reference <the Pro-C 2 path>`. If it does, §6 is
   confirmed as a host limitation rather than something specific to Pro-C 3;
   if it does not, the cause is something else and worth knowing.

A real project holding Pro-C 2 instances is also useful to copy back — a
`--dry-run` over it exercises decode against instances the factory presets do
not cover. (The one already on this machine,
`02 LORD OF THE FIGHT.RPP`, carries six.)

---

## 8. What already exists

| Tool | What it answers |
|---|---|
| `signal-plugin-host` / `load_plugin` | does it load, what is its id, how many parameters |
| `signal-import` / `ffp_survey` | parameter names, order, layout variants, what varies |
| `signal-analyzer` / `plugin_params` | what a stored value means, as a table or a curve |
| `signal-analyzer` / `eq_bundle_check` | does the installed EQ bundle match the engine |
| `signal-analyzer` / `comp_bundle_check` | do the two compressors pass audio, and at what level |
| `signal-analyzer` / `eq_match`, `eq_sweep` | one preset, and a whole library, against the plugin |
| `fts-convert --verify --engine-too` | every instance in a project, both columns |

Decoders live in `src/fabfilter/`, the project surgery in `src/rpp/`, and the
parameter maps onto FTS plugins in `src/rpp/fts_*.rs`.
