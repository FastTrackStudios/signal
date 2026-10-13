# Song-folder format

The **Song folder** is the durable, portable storage form of a song: a
self-contained directory of plaintext (`---`-fenced Markdown frontmatter,
the same convention the Task vault uses) plus references to attachments.
It is the *stored* counterpart to the runtime `session_proto::Song`, which
is hydrated from this form later (out of scope for this crate).

A song is **keyflow-agnostic**: charts are stored as *references* (a
relative path within the song folder and/or an attachment id), never as an
embedded `keyflow::Chart`. Parsing/rendering charts is a separate
workstream.

## Layout

```text
<song-root>/
  song.md                         # index: identity + arrangement list + default pointer
  arrangements/
    <arr-dir>/                     # one folder per arrangement (slug of its name)
      arrangement.md               # authoritative full arrangement record
      chart.kf                     # (optional) chart bytes, referenced by chartRef — not written by this crate
      ...                          # (optional) per-part / attachment files
```

- `<song-root>` folder name is cosmetic. Identity is the `id` in
  `song.md`, never the folder name — renaming the folder does not orphan
  references.
- `<arr-dir>` is a slug of the arrangement name (`Default` → `default`).
  Collisions are disambiguated with a short id suffix (`default-1a2b3c4d`).

## `song.md` — the index

Frontmatter only (no body today). Fields are camelCase.

```markdown
---
id: 5f2c…                          # SongId (UUID), stable identity
title: Great Are You Lord
tags:
- worship
- set-a
defaultArrangement: 9ab1…         # ArrangementId of the default arrangement
arrangements:                      # ordered list; order is preserved on read
- id: 9ab1…
  name: Default
  dir: default                     # folder under arrangements/
  key: Bb Major                    # convenience mirror (authoritative copy is in the record)
- id: 3c7d…
  name: Acoustic
  dir: acoustic
  key: G Minor
---
```

`song.md` carries the identity, the tag set, the **default arrangement**
pointer, and the ordered arrangement list with lightweight metadata (name,
folder, key) — enough to list a library without opening every arrangement
record. The `key` here is a convenience mirror; the authoritative value
lives in the arrangement record and is what a read returns.

## `arrangements/<dir>/arrangement.md` — the arrangement record

The authoritative, full record for one arrangement. Frontmatter only.

```markdown
---
id: 9ab1…
name: Default
key: Bb Major                      # root (letter + accidental) + mode, e.g. `F# Dorian`, `C` (bare = Major)
chartRef:                          # optional; omitted entirely when absent
  path: arrangements/default/chart.kf
  attachmentId: null               # optional durable pointer
parts:
  parts:
  - name: Lead Vocal
    resourceRefs:
    - path: arrangements/default/lead-vocal.md
      kind: chart
  - name: Click
    resourceRefs: []
attachmentRefs:
- id: att-001
  path: attachments/reference.mp3
  sha256: deadbeef
  kind: audio
---
```

### Field semantics

| Field             | Meaning                                                                 |
|-------------------|-------------------------------------------------------------------------|
| `id`              | `ArrangementId` (UUID), stable within the song.                         |
| `name`            | Human name (`Default`, `Acoustic`, `Key of A`).                         |
| `key`             | Musical key: tonic letter + accidental + mode. Stored as a compact string (`Bb Minor`, `F# Dorian`, `C`). A bare root means Major. |
| `chartRef`        | Optional chart **reference**: a relative `path` and/or `attachmentId`. Never an embedded chart. |
| `parts`           | `PartsManifest` — an open, org-defined set of parts, each with a name and resource references. Minimal today; part-filtered views are a later workstream (W5). |
| `attachmentRefs`  | References to attachment files (audio, PDF, image). `id` + optional `path` / `sha256` / `kind`. The real `AttachmentService` is wired elsewhere; these are references only. |

## Reference types

- **ChartRef** — `{ path?, attachmentId? }`. At least one set; when both
  are present `path` is authoritative and `attachmentId` is a durable
  pointer that survives moves.
- **AttachmentRef** — `{ id, path?, sha256?, kind? }`. A reference to a
  file that belongs to the song; no I/O or service coupling here.
- **ResourceRef** — `{ path?, attachmentId?, kind? }`. Used inside a
  `Part` for its associated resources.

## Round-trip guarantees

`song::to_folder(&song, root)` then `song::from_folder(root)` returns a
`Song` equal to the original. `song.md` fixes arrangement **order** and the
**default** pointer; each `arrangement.md` is the source of truth for that
arrangement's fields (including `key`).
