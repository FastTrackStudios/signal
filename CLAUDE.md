# Signal — Claude Code Instructions

Signal is the signal chain / plugin management domain for FastTrackStudio.

## Architecture

This repo is being restructured into the **feature-slice layout** of
[architect](https://codeberg.org/FastTrackStudios/architect) — crates are
grouped under top-level `features/<name>/` folders. See `DESIGN.md` for the
full target shape and the staged migration plan.

```
features/
  signal/     # domain slice: signal-proto, signal-storage, signal-live,
              #   signal-controller, signal-import, signal-daw-bridge,
              #   signal (facade), signal-ui, signal-browser
  sampler/    # signal-sampler, signal-sampler-clap
  plugin-host/# signal-plugin-host
  nam/        # nam-manager
  macromod/   # macromod
  reaper-integration/  # signal-extension, fts-signal-controller
apps/         # cli · desktop · mobile · native · tui
```

The `signal` facade is still the only public API surface: apps depend only on
`signal` (facade), `signal-ui`, or `signal-sampler`, never on the internal
domain crates (`signal-proto`, `signal-controller`, `signal-live`,
`signal-storage`, `signal-import`, `signal-daw-bridge`, `nam-manager`).

**Dependency rule**: all intra-workspace deps are `x.workspace = true`; only
the root `Cargo.toml` `[workspace.dependencies]` table carries paths.

> The monolithic `features/signal/` slice is split by bounded context
> (`tone` / `rig` / `perform`) and migrated onto `#[architect::Entity]` in
> later phases — see `DESIGN.md`. `crates/signal-daw` remains parked.

## GUI Architecture

Signal's UI must render identically in three contexts:
1. **Standalone desktop app** (`signal-desktop`)
2. **VST3/CLAP plugin** (`fts-signal-plugin`, Phase 7)
3. **Embedded in REAPER** (existing `fts-signal-controller`)

To guarantee this, **all three contexts must use the same rendering pipeline**:
`nih_plug_dioxus` → Blitz (Vello + wgpu) → baseview.

### Rules

- **`signal-desktop` MUST use `nih_plug_dioxus::open_standalone(App, w, h)`** —
  never `dioxus::desktop::LaunchBuilder`. The standard Dioxus desktop renderer
  (WebKit/WRY) is a different pipeline and breaks VST compatibility.
- **No `dioxus = { features = ["desktop"] }` in `signal-desktop`** — the Dioxus
  runtime comes from `nih_plug_dioxus`; direct `dioxus` dep is only needed for
  `dioxus::prelude::*` (no feature flags required for that).
- **Inline styles only in `signal-ui`** — Blitz does not load external CSS files
  reliably. Use inline `style="..."` attributes, or embed CSS as a static string
  via `document::Style { {CSS_STR} }`. Never `document::Stylesheet { href: ... }`.
- **`signal-ui` components must render correctly without Tailwind** — use explicit
  style values, not Tailwind class names, for any layout-critical properties.
  Tailwind classes may be used additively but must not be load-order dependent.
- **No Tailwind `asset!()` calls in components** — embed the CSS via
  `include_str!()` into a `&'static str` constant that the root component inlines
  via `document::Style`.
- The root `App` component must accept no props (so it works in both
  `open_standalone(App, w, h)` and `create_dioxus_editor_with_state(state, state, App)`).
  Pass runtime state via Dioxus context (`use_context_provider` / `use_context`).

### Dependency pattern

```toml
# signal-desktop/Cargo.toml
nih_plug_dioxus.workspace = true   # Blitz renderer + window creation
dioxus.workspace = true            # dioxus::prelude — NO feature flags needed
signal.workspace = true
signal-ui.workspace = true
```

### Entry-point pattern

```rust
// signal-desktop/src/main.rs
fn main() {
    // Blocking init (DB, audio engine) before opening the window.
    // Use a single-shot tokio runtime — done before Blitz takes the thread.
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let controller: Signal = rt.block_on(connect_db_seeded(&db_path)).unwrap();

    // Wrap in Arc so SharedState can type-erase it.
    let shared = nih_plug_dioxus::SharedState::new(Arc::new(controller));

    // Open native Blitz window. Blocks until closed.
    nih_plug_dioxus::open_standalone_with_state(App, 1400, 900, Some(shared));
}

#[component]
fn App() -> Element {
    // Retrieve state injected by open_standalone_with_state.
    let shared = use_context::<SharedState>();
    let controller = shared.get::<Signal>().expect("Signal not in context");

    rsx! {
        // Embed Tailwind — never use document::Stylesheet { href: ... }
        document::Style { {nih_plug_dioxus::TAILWIND_CSS} }
        SignalRoot { controller: (*controller).clone() }
    }
}
```

### Plugin entry-point pattern (Phase 7, fts-signal-plugin)

```rust
fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
    create_dioxus_editor_with_state(
        self.editor_state.clone(),   // Arc<DioxusState>
        self.ui_state.clone(),       // Arc<SignalUiState>
        App,                         // same App component as standalone
    )
}
```

## Platform Targets

Signal must run across three environments. All processing-core crates must
support all three; only I/O adapter crates are platform-specific.

| Target | Crates | Notes |
|---|---|---|
| **Native** (Linux/macOS/Pi) | all | Full `std`, JACK/ALSA/CoreAudio via `cpal` |
| **WASM / Browser** | processing core | AudioWorklet drives `AudioGraph::process()`; no `cpal` |
| **Embedded `no_std`** | processing core | `#![no_std]` + `alloc`; no OS, no threads |

### Processing-core crate rules (`daw-audio-graph`, `daw-builtin-fx`, `signal-audio`)

- `#![no_std]` compatible — depend only on `core` and `alloc`, never `std`
  directly. Where `std` is unavoidable (e.g. trait objects need `alloc`),
  gate it behind `#[cfg(feature = "std")]` and keep the `std` feature
  additive/default.
- **No heap allocation on the hot path** — `BufferPool` pre-allocates at
  `reset()` time; `process()` must never call `Vec::push`, `Box::new`, or
  any allocator directly.
- **No threads** — the graph is driven synchronously by whichever callback
  owns it (cpal, AudioWorklet, bare-metal ISR). Never call
  `moire::task::spawn` or `std::thread::spawn` inside processing crates.
- **No platform I/O** — no `cpal`, no `web-sys`, no MIDI drivers inside
  the graph. I/O lives exclusively in adapter crates (`signal-desktop`,
  `signal-web`, `signal-embedded`).
- **`AudioNode: Send`** — keep the `Send` bound; it's auto-satisfied in
  single-threaded WASM and needed for multi-threaded WASM / native.

### Audio I/O adapter crates (platform-specific, not in processing core)

- **Native**: `cpal` with `jack` feature — JACK when available, ALSA/CoreAudio
  fallback. Lives in `signal-desktop`.
- **Browser**: `web-sys` AudioWorklet bindings + `wasm-bindgen`. Lives in
  `signal-web` (future crate).
- **Embedded**: bare-metal DMA callback. Lives in `signal-embedded` (future).

## Key Rules

### Async & Concurrency
- Use `moire::task::spawn` instead of `tokio::spawn`
- Use `moire::sync::Mutex` / `moire::sync::RwLock` instead of tokio/std equivalents
- Never hold std sync primitives across `.await`

### RPC Services
- Service traits use `#[vox::service]`
- Max 4 params per method (Facet constraint)
- Use `Tx<T>` / `Rx<T>` for streaming

## Build & Test

```bash
cargo check -p signal           # Type-check facade
cargo check --workspace         # Type-check all
cargo test -p signal            # Run tests
```

## Issue Tracking

Use `bd` (beads) for all task tracking. See AGENTS.md for workflow.
