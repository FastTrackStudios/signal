# Processor — Repo Instructions

**This repo is the DSP, the effects, and the plugins.** Split out of
`signal` in September 2026 so that signal and session can both use the
effects without depending on each other.

| repo | holds | consumed as |
|---|---|---|
| **processor** (here) | `features/fx/*`, `features/preset-browser`, the CLAP/VST3 plugins in `apps/plugins`, the standalone host, the bundler | — |
| [signal](https://github.com/FastTrackStudios/signal) | the signal domain: sampler, rigs, NAM, plugin-host, the Signal app | takes this as a git dep |
| [session](https://github.com/FastTrackStudios/session) | setlists, songs, the guide | takes this as a git dep |
| [daw](https://github.com/FastTrackStudios/daw) | the daw domain, `fts-audio-ui`, `fts-plug-ui`, `dioxus-test` | git dep, tag |
| [architect](https://github.com/FastTrackStudios/architect) | the framework, `architect-ui` | git dep, tag |

One workspace, one lockfile, one `target/`, one flake. Intra-repo
dependencies are path deps in `[workspace.dependencies]`. Cross-repo
dependencies are **git deps pinned to a tag**.

## The rule that defines this repo

**Nothing here may depend on signal or session.** If an effect needs
something from a product domain, the dependency is pointing the wrong
way — the product should adapt to the effect, not the other way round.
`fx-blocks` is the example to follow: it wraps the FX in `daw`'s
`PluginInstance` contract, which both products already speak, rather than
in either product's own.

That is also why NAM is not here. It reads as an effect and is not one:
it needs `signal-proto`, `signal-space`, `signal-sampler` and the
Tone3000 model browser. It stays in signal until that is worth unpicking.

## Crate shape

Each effect is up to four crates, and the split is load-bearing:

- `*-dsp` — the engine. `no_std` + `alloc` compatible, no heap on the hot
  path (pre-allocate in `reset()`; `process()` never calls `Vec::push`),
  no threads, no platform I/O. These have to build for native, WASM and
  embedded.
- `*-profiles` — the voicings. Data, not behaviour.
- the facade (`eq`, `comp`, …) — the chain a host actually runs.
- `*-ui` — the face. Depends on dioxus/blitz; never depended on by DSP.

Apps depend on the facade or the UI, never on a `-dsp` crate directly.

## GUI rules

Plugin UI must render identically standalone, as a VST3/CLAP plugin, and
embedded in a DAW, so every context shares one pipeline:
`nice-plug-dioxus` → Blitz (Vello + wgpu) → baseview.

- Never `dioxus::desktop::LaunchBuilder` — WebKit/WRY breaks VST parity.
- **Inline styles only.** Blitz does not load external stylesheets. Inline
  `style="..."`, or embed CSS as a static string via `document::Style`;
  never `document::Stylesheet { href: .. }`, no Tailwind `asset!()`.
- Components must render correctly without Tailwind — explicit values for
  anything layout-critical. Tailwind classes are additive only.
- Root `App` components take no props (context via `use_context_provider`)
  so the same component serves standalone and as a plugin editor.
- Blitz has gaps worth knowing before you fight one: `use_effect` never
  runs inside a plugin, `autofocus` is ignored unless the node is already
  in the document, `MountedData::set_focus` panics from an event handler,
  and there is no `placeholder`. `eq-ui`'s band-rename field documents
  all four at the call site.

## Parameters

Every parameter must parse the string it prints — a host's parameter list
is text in, text out, and one that formats but does not parse is
read-only in every DAW. `param_round_trip.rs` in each `-ui` crate holds
that line; it is the test that caught 41 percentage parameters with no
unit and no parser.

Time controls that can lock to tempo use `musical-time`. Do not grow a
second table of note values.

## Logging

**Never `println!`/`eprintln!`/`dbg!` in library code** — not committed,
not as scaffolding. Reproduce a bug in a failing test instead. The plugin
binaries in `apps/plugins` may print; libraries may not.

## Build

```bash
cargo check -p eq-plugin -p comp-plugin -p saturate-plugin -p delay-plugin -p reverb-plugin
just plugins-bundle          # .clap and .vst3 into target/bundled
just plugins-link            # symlink them so a rebuild is live in a DAW
just host eq                 # open one in the standalone host
```

`cargo check --workspace` pulls PipeWire in through `daw`'s audio backend
and fails on macOS. Inherited, not new.

## Licence

GPL-3.0-or-later; a new crate inherits it via `license.workspace = true`.
The two crates under `vendor/` keep the licence of the code they vendor —
each has an `FTS-PATCH.md` saying what was changed and why.
