# Dioxus Renderer Event Converters

DAW can host Dioxus native/Blitz panels and embedded Dioxus desktop/WebKit
panels in the same REAPER process. Dioxus currently stores the HTML event
converter in one global `dioxus_html` slot, so whichever renderer last called
`set_event_converter` controls how later `PlatformEventData` is downcast.

That global converter is not renderer-safe:

- Native Blitz events carry native pointer, keyboard, focus, and mounted data.
- Desktop/WebKit embedded events carry serialized HTML event payloads.
- If the wrong converter is active, event dispatch can panic while unwrapping
  the expected event payload type inside an extern callback.

## Local Contract

Until Dioxus exposes scoped or runtime-local event converter handling, every
renderer entry point that dispatches an event must reassert the converter it
requires immediately before handing the event to Dioxus:

- Blitz native dispatch sets `NativeConverter` before converting native DOM
  events.
- Embedded desktop dispatch sets `dioxus_html::SerializedHtmlEventConverter`
  before delivering decoded webview events to the desktop runtime.

This is intentionally done at dispatch time, not only at renderer startup.
Startup-only registration is insufficient because multiple renderers can be
mounted concurrently and event callbacks interleave on the same process-global
converter.

## Patch Locations

The DAW workspace currently relies on local sibling checkouts for the renderer
patches:

- `/home/cody/Development/FastTrackStudio/blitz/packages/dioxus-native-dom/src/dioxus_document.rs`
  reasserts `NativeConverter` in `DioxusDocument::new` and before native event
  dispatch.
- `/home/cody/Development/Dioxus/dioxus/packages/desktop/src/embedded.rs`
  reasserts `dioxus_html::SerializedHtmlEventConverter` before embedded desktop
  webview event dispatch.

The DAW `Cargo.toml` patches Dioxus and Blitz to those local sibling checkouts.
If those patches are removed or replaced with upstream crates, mixed native and
desktop panels must be revalidated before enabling `fts-ui-desktop`.

## Upstream Direction

The upstreamable fix should make the converter renderer-specific instead of
process-global. Viable shapes:

- store the converter on the Dioxus runtime or virtual DOM and use it from event
  dispatch;
- add a scoped converter guard around dispatch APIs;
- make `Event` carry a converter or typed platform payload that avoids global
  downcast state.

The important invariant is that one renderer must not be able to change how
another renderer interprets already-created event payloads.
