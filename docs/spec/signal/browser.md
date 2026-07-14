# Browser

One faceted, tag-driven browser over **every** signal artifact — a single index
and query engine that browses Block/Module/Layer/Engine/Rig presets, Profiles,
Songs, and Setlists (the [hierarchy](hierarchy.md)) through the same UI. v1 scope
is instrument presets (soundsource packs + rig presets); it expands to the
performance levels without a new mechanism. Reference: `crates/signal/browser`
and `crates/signal/proto/src/tagging.rs` (`BrowserIndex`, `StructuredTag`,
`browser_columns`).

## Index

r[signal.browser]
The browser presents a searchable, filterable view of preset artifacts. It is
backend-agnostic (it browses `.signalpack`/`.signal.styx` sidecars and rig
presets, not REAPER files specifically) and drives loading into a target slot.

r[signal.browser.index]
The browser is backed by a `BrowserIndex`: one `BrowserEntry` per artifact,
carrying its stable id, kind, display name, and its `TagSet`. The index is built
by scanning preset sidecars + pack headers (`read_pack_header`, no audio decode)
and is rebuildable/incremental.

r[signal.browser.entity-kind]
Each entry has a `BrowserEntityKind` distinguishing a **Collection** (a family,
e.g. a preset that has variants/snapshots) from a **Variant** (one concrete
choice), at each hierarchy level (Block/Module/Layer/Engine/Rig/Profile/Song/
Setlist). Browsing a collection drills into its variants.

## Tags

r[signal.browser.tag]
Every entry is tagged with `StructuredTag { category, value, source, weight }`.
Categories are a fixed set: rig type, engine type, domain level, instrument, tone,
character, genre, context, module, block, vendor, plugin, workflow, custom.
Multiple values per category are allowed; weight orders relevance.

r[signal.browser.tag-source]
A tag records its **source**: Manual (user), InferredName, InferredStructure,
Imported (from the original library metadata), or System. Sources let the UI
trust/override inferred tags and let re-scans replace inferred tags without
clobbering user tags.

r[signal.browser.tag-weights]
A `TagWeights` map assigns per-category weight, so a query ranks by weighted tag
overlap (a matching `instrument` tag may outweigh a matching `context` tag). Tag
enrichment quality directly drives facet usefulness.

## Query & navigation

r[signal.browser.query]
A `BrowserQuery` filters the index by free text (name + tag values), by required
tag facets (category=value constraints), and by kind, returning ranked
`BrowserHit`s. Faceting is additive — selecting a facet narrows the visible set
and updates the remaining facet counts.

r[signal.browser.modes]
The browser offers navigation **modes** that pick which facet columns lead:
Semantic (instrument/tone/character…), Vendor (vendor→plugin→tone), Genre, and
Performance (context/workflow…). The active column set adapts to the mode and the
rig type (`browser_columns`).

r[signal.browser.columns]
The browser navigates as ordered facet **columns** (Miller-column / tree style):
each column is a tag category, selecting a value filters the next column, and a
detail panel shows the focused entry (name, tags, params, source library).

## Audition & load

r[signal.browser.audition]
An entry MAY be auditioned before committing — preview its sound and inspect its
parameters/sidecar without fully loading it into the live graph. Favorites and
ratings are user tags (`TagSource::Manual`).

r[signal.browser.load]
Selecting an entry loads it into the **target slot** implied by its kind: a Block
preset into a Block slot, a soundsource pack into a Layer's source, a Profile as
the active profile, etc. Loading uses the live-parameter path where possible and
MUST NOT re-host the running graph (see `signal.instrument.control.live`).

## Scope

r[signal.browser.scope]
v1 indexes **instrument presets** — soundsource packs and rig/engine/layer
presets. The same index/query/UI extends to Profiles, Songs, and Setlists (all
already `BrowserEntityKind`s) and later to arbitrary tracks, with no new browser
machinery — only new entry sources and tags.
