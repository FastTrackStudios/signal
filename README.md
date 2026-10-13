# studio

FastTrackStudio's audio and music software, in one repository: a DAW and
the apps built on it.

- **Session** — the full DAW: tracking, editing and mixing, setlists and
  songs, charts in the keyflow language, and the guide (click, count-in,
  spoken section cues). Desktop, iPhone, and a browser version.
- **Signal** — a live instrument rig: guitar, bass, keys, drums and
  sampled instruments, with tones and patches switched during a set. The
  rig runs headless and every screen — laptop, phone, tablet, browser — is
  a remote attached to it.
- **Patchbay** — studio routing: PipeWire, Core Audio, Dante and digital
  desks, wired and recalled from one window.
- **Plugins** — the FTS effects (EQ, compressor, saturation, delay,
  reverb, modulation, gate, limiter, pitch, NAM amps and more) as CLAP and
  VST3.
- **frame** — the design and control-surface engine the plugin faces and
  control surfaces are drawn with.

Session and Signal run the same engine: one DAW engine per process, with
a rig being a folder of tracks in it
([docs/design/one-engine.md](docs/design/one-engine.md)).

## Layout

```
apps/      the applications — session/, signal/, patchbay/, plugins/, themer/
crates/    daw (the DAW core and its standalone engine), studio (the
           umbrella: REAPER adapter, project files, UI, CLI), frame, keyflow
features/  one crate per feature — the effects, the rigs, audio, MIDI,
           formats, editing, control surfaces, …
vendor/    patched third-party crates
xtask/     build tooling
docs/      design notes and guides
```

## Building

Everything builds from the repository root, in the Nix dev shell:

```bash
nix develop                 # or: direnv allow
just check                  # cargo check --workspace
just test                   # the test suite (cargo nextest)
just app                    # run Session
just guitar                 # run Signal's guitar rig
just plugins-bundle         # build the CLAP/VST3 plugins into target/bundled
```

`just --list` shows every recipe.

## Assets

Large binaries — test fixtures, research captures, the amp captures and
impulse responses the rigs ship with — live in separate private
repositories. `just assets` fetches them and links them into place
(the list is in `assets.txt`). The tests that read those fixtures, and
the rigs' bundled models, need them.

## Licence

GPL-3.0-or-later. Vendored third-party code under `vendor/` keeps its
own licence.
