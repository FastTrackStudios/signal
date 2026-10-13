# Agent Guide

This repository is a Dioxus 0.7 UI library managed through Nix flakes. Treat
`dioxus-flake` as the source of truth for Dioxus tooling and platform
dependencies.

## Hard Requirements

- Use Dioxus 0.7 APIs only.
- Do not use removed Dioxus APIs such as `cx`, `Scope`, or `use_state`.
- Prefer `use_signal`, `use_memo`, `use_resource`, `use_context_provider`, and
  `use_context`.
- Components take owned props (`String`, `Vec<T>`, etc.) and props should be
  `Clone + PartialEq`.
- Keep component styling compatible with Tailwind class merging through
  `crate::cn::merge_slice`, `crate::cn::merge`, or `architect_ui::cn!`.
- Prefer `dioxus_primitives` for behavior and accessibility, then wrap the
  primitive with architect-ui styling.
- Do not hand-roll accessibility behavior if a Dioxus primitive exists.

## Repository Shape

- `crates/fts-ui`: reusable component library.
- `crates/showcase`: older showcase crate kept for compatibility.
- `apps/web`: web showcase app.
- `apps/desktop`: desktop/webview showcase app.
- `apps/mobile`: Android/mobile showcase app.
- `apps/native`: native renderer showcase app.
- `flake.nix`: development shells and package build.

All `apps/*` packages should render `architect_ui::showcase::Showcase` unless the
task explicitly asks for platform-specific experiments.

## NixOS / Flake Workflow

Enter the normal development shell:

```sh
nix develop
```

The default shell inherits from `dioxus-flake.devShells.default` and provides
Rust, Dioxus CLI, desktop/web/native dependencies, and these aliases:

```sh
fts-build
fts-test
fts-check
fts-clippy
fts-showcase-web
fts-showcase-desktop
fts-showcase-native
```

Use the mobile shell for Android SDK/emulator work:

```sh
nix develop .#mobile
fts-build-mobile
fts-showcase-mobile
```

Keep Android-specific SDK/emulator assumptions out of the default shell. Mobile
work belongs in `.#mobile`.

## Verification

For most component changes, run:

```sh
cargo fmt --check
cargo check -p fts-ui-showcase-web
cargo test -p architect-ui
cargo clippy --workspace -- -D warnings
git diff --check
```

For broader shared changes, run:

```sh
cargo check --workspace
```

If touching native/mobile renderer behavior, also check the relevant app:

```sh
cargo check -p fts-ui-showcase-native
cargo check -p fts-ui-showcase-mobile
```

## Running Showcases

From `nix develop`:

```sh
fts-showcase-web
fts-showcase-desktop
fts-showcase-native
```

From `nix develop .#mobile`:

```sh
fts-showcase-mobile
```

The web showcase usually serves on the Dioxus CLI's reported local URL. Do not
assume a fixed port if `dx` selects a different one.

## Bootstrapping a New App in This Workspace

Create a new directory under `apps/<name>` with:

```toml
[package]
name = "fts-ui-showcase-<name>"
version.workspace = true
edition.workspace = true

[dependencies]
dioxus = { workspace = true, features = ["web"] }
architect-ui.workspace = true

[lints]
workspace = true
```

Adjust the Dioxus feature for the target:

- web: `features = ["web"]`
- desktop: `features = ["desktop"]`
- mobile: `features = ["mobile"]`
- native renderer: `features = ["native"]`

Create `Dioxus.toml`:

```toml
[application]
name = "fts-ui-showcase-<name>"
default_platform = "web"

[web.app]
title = "architect-ui Showcase"
```

Create `src/main.rs`:

```rust
use dioxus::prelude::*;
use architect_ui::showcase::Showcase;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Stylesheet { href: TAILWIND_CSS }
        Showcase {}
    }
}
```

Add `assets/tailwind.css`. For platform showcases, keep it generated from the
same source conventions as the existing `apps/*/assets/tailwind.css` files.

## Bootstrapping a Consumer App Outside This Repo

Use the same `dioxus-flake` dependency chain as this repo. In the consumer
flake, follow `dioxus-flake` for `nixpkgs` and inherit the relevant
`dioxus-flake.devShells.${system}.default` or `.mobile`.

In `Cargo.toml`, depend on architect-ui by path during local development:

```toml
[dependencies]
dioxus = { version = "0.7.6", default-features = false, features = ["web"] }
architect-ui = { path = "../fts-ui/crates/fts-ui" }
```

For workspace-based apps in FastTrackStudio, prefer:

```toml
architect-ui.workspace = true
```

## Theme System

Use `ThemeProvider` for app-level runtime themes and `ThemeScope` for nested
regions that need separate colors, typography, radius, and shadows.

```rust
let theme_state = use_signal(default_theme_state);

rsx! {
    ThemeProvider { state: theme_state,
        ThemeSwitcher { state: theme_state }
        ThemeScope {
            styles: theme_preset("doom-64").unwrap().styles,
            mode: Some(ThemeMode::Dark),
            // child UI
        }
    }
}
```

By default, theme presets do not alter Tailwind `--spacing`, because that
resizes layout utilities such as `gap-*`, `px-*`, `size-*`, and component icon
sizes. Set `allow_layout_scale: true` on `ThemeProvider` or `ThemeScope` only
when an app intentionally wants a theme to resize layout.

## Component Rules

- Use lucide icons from `lucide-dioxus` instead of custom SVGs when an icon
  exists.
- Components should accept `class: String` and merge it last.
- If Tailwind arbitrary variants are not present in generated CSS, prefer a
  small component-scoped `document::Style` rule over relying on missing classes.
- Keep default components shadcn-like, but respect Dioxus primitives and native
  renderer constraints.
- `Select` should use the custom architect-ui `Select`, not raw HTML `<select>`,
  unless the component is intentionally named `NativeSelect`.
- Raw HTML `<select>` belongs only in low-level/native-select cases.

## Git Hygiene

- Do not commit `.direnv/`.
- Do not revert user changes unless explicitly requested.
- Keep commits focused and verify before committing.

