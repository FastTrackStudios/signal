# session

Post-production and live-performance coordination: setlists, songs,
charts, the keyflow chart language, notation, and the guide (click,
count-in, spoken section cues).

Split out of the FastTrackStudio monorepo in August 2026.

## Where this sits

Session is the **coordinator at runtime** — the Session app opens and
syncs Signal and Ignition over WebSocket, and depends on neither.

In *cargo* terms the arrow is the other way round: this repo publishes
the musical vocabulary (`keyflow`, `song`, `session-proto`,
`session-ui`, `engraver`) that Signal's rigs consume, so it sits
**below** `signal` in the dependency graph. Runtime role and layer
order are deliberately opposite; the graph stays acyclic because the
coordination is over the wire, not over cargo.

```
daw  ->  session  ->  signal
             ^
             └── the Session app lives here too, and talks to
                 Signal / Ignition over ws:// rather than linking them
```

## Layout

```
crates/session/      the session domain — setlists, transport, proto, ui
crates/keyflow/      the keyflow chart language — syntax, chordpro,
                     musicxml, midi, live, sync, lsp, tree-sitter, ui
features/song/       song sections and arrangement
features/engraver/   MusicXML -> scores and parts
features/guide/      click / count-in / spoken section cues
features/chord-tool/ chord entry and analysis
features/dynamic-template/  dynamic session templates
apps/desktop/                the Session app
```

## Build

```bash
nix develop
cargo check --workspace
```

## Licence

GPL-3.0-or-later.
