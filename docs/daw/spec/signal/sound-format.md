# Sound Definition Format

The plaintext file format that describes an entire instrument — its mapping,
round-robins, mics, articulations, layers, block chains, presets, and
modulation — **everything except the audio itself**. Audio bodies live in binary
[`.soundpack`](sampling.md) slices; this format is the human- and LLM-readable
brain that references them.

This format is the differentiator: because the whole instrument (minus audio) is
plaintext, it is **queryable, editable, saveable, shareable, diffable, and
git-trackable**. It builds on the project's [styx](../styx-guide.md) syntax and
generalizes the existing `library.styx` (zone map) and `.signal.styx` (preset
sidecar) into one coherent, specced format.

## Principles

r[signal.format.plaintext]
Everything about a sound except its audio is stored as **plaintext**. The audio
lives in `.soundpack` bodies (binary, not human-readable); everything else —
mapping, round-robins, mics, articulations, layers, blocks, presets, modulation,
tags — is text. No instrument behavior is encoded in a binary blob that a human
or an LLM cannot read and edit.

r[signal.format.styx]
The concrete syntax is **styx** (facet-styx): `key value` fields, sequences
`(a b c)`, nested blocks `{ … }`, variant tags `@name{ … }`, quoted strings, and
comments. It maps 1:1 to the domain's Rust types so the same file both documents
and loads the instrument.

r[signal.format.readable]
Values are expressed in **human-meaningful** units and names — note names (C3,
F#4), Hz, dB, ms, semitones, percentages — never opaque indices or raw
byte-offsets. A person or an LLM reads and edits *intent* directly (e.g.
`cutoff 800hz`, not `param_37 0.4213`).

r[signal.format.canonical]
The **text is the source of truth**. Tooling MUST edit it non-destructively:
preserve comments, key order, and fields it does not understand; never emit
spurious defaulted fields (the styx serializer's habit of writing defaulted
`Option`s as variant tags corrupts the file, so edits inject into the existing
text rather than blindly re-serialize). Output ordering is deterministic for
clean line-diffs.

r[signal.format.reference]
Everything nameable — samples, zones, mics, articulations, parameters, blocks,
presets — has a **stable id/name** and is referenced by it, so edits, links, and
modulation routes survive reordering and re-export (ties to
`signal.parameter.persist`).

## Files & structure

r[signal.format.folder]
A sound is a **folder**: one manifest (e.g. `sound.styx`, the role today's
`library.styx` plays) plus its `.soundpack` audio slices, plus an optional derived
browse sidecar (`.signal.styx`). The manifest is self-contained and references its
packs by relative path; the folder is the shareable/git-trackable unit.

r[signal.format.identity]
The manifest header carries the browsable metadata: schema `version`, a stable
UUID `id`, `name`, `kind` (`signal.preset.kind`), `tags`
(`signal.browser.tag`), `description`, and author/license/attribution. The
[browser](browser.md) sidecar is derivable from this header.

r[signal.format.packs]
The manifest **declares its soundpack slices**: each `(mic, mix, layer)` slice
maps to a `.soundpack` relative path + content hash, and lists which samples/zones
that slice holds — so the loader streams only enabled slices
(`signal.sampling.soundpack.optional`) and can verify integrity.

r[signal.format.modular]
A large library MAY **split** its definition across multiple included styx files
(e.g. one per articulation) that the manifest references, so files stay small,
diffs stay local, and several editors/LLMs can work in parallel.

## Content

r[signal.format.keymap]
Zones are declared as data (`signal.sampling.zone`): key range + root, velocity
window + crossfade, tune, gain, pan, sample-start, loop points, and trigger mode —
one zone per entry, each referencing a sample by name within a pack slice.

r[signal.format.roundrobin]
Round-robin sets are explicit (`signal.sampling.roundrobin`): a named set groups
zones sharing a (key, velocity, articulation) window, with a cycling mode
(`cycle` / `random` / `no-repeat`).

r[signal.format.groups]
Mics, articulations, variations, groups, dynamics, and their selectors
(keyswitch / CC / velocity) are all named text entries
(`signal.sampling.multimic`, `signal.sampling.articulation`,
`signal.sampling.dynamics`) — the whole switching model is readable and editable.

r[signal.format.layers]
Nestable [layers](hierarchy.md) are data: each layer's source, keyboard zone, and
its **block chain** — filter/amp/FX as Blocks in Modules (`signal.hierarchy.uniform`)
— with every block's parameters written as named values.

r[signal.format.presets]
Presets and snapshots are named parameter sets in the same file (or included
files): multiple presets, profiles, scenes, and stacks (`signal.profile`,
`signal.stack`) reference the shared blocks/samples by id rather than copying
state.

r[signal.format.modulation]
[Modulator](modulator.md) routes, [macros](macro.md), and parameter links are
declared by stable `ParamTarget` address: `source → target`, amount, and curve —
so the full control graph is plaintext and editable alongside everything else.

## Interop

r[signal.format.query]
Because the definition is plaintext styx with stable ids and structured tags, it
is **queryable and mergeable without loading audio**: the browser indexes it, an
LLM edits it, `grep`/diff/merge operate on it, and it is shareable as text.

r[signal.format.import]
Third-party libraries — Omnisphere `.prt_omn`, Kontakt, SFZ, filename-convention
libraries (CSS) — **import into** this format; the styx manifest becomes the
canonical, editable representation, decoupling the sound from its origin tool.
