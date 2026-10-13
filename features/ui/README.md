# architect-ui

FastTrack Studio's Dioxus UI library. It provides shadcn-style components for
Dioxus 0.7 across web, desktop, mobile, and native renderer targets.

## Development

Use the flake shell so Dioxus, Rust, and platform dependencies come from the
shared `dioxus-flake` input:

```sh
nix develop
```

Useful aliases inside the shell:

```sh
fts-check
fts-clippy
fts-test
fts-showcase-web
fts-showcase-desktop
fts-showcase-native
```

Android/mobile work uses the separate mobile shell:

```sh
nix develop .#mobile
fts-showcase-mobile
```

## Showcase Apps

Each platform showcase lives under `apps/` and renders
`architect_ui::showcase::Showcase`:

- `apps/web`
- `apps/desktop`
- `apps/mobile`
- `apps/native`

The showcase is the fastest way to verify component styling, theme behavior,
and platform-specific rendering differences.

## Component Library

The reusable library is in `crates/fts-ui`.

Typical app usage:

```rust
use dioxus::prelude::*;
use architect_ui::prelude::*;

#[component]
fn App() -> Element {
    let value = use_signal(String::new);

    rsx! {
        Button { "Save" }
        Input { value, placeholder: "Name".to_string() }
    }
}
```

Theme support is runtime-driven:

```rust
let theme_state = use_signal(default_theme_state);

rsx! {
    ThemeProvider { state: theme_state,
        ThemeSwitcher { state: theme_state }
        // app UI
    }
}
```

Use `ThemeScope` for app regions that need separate theme context.

