# raw-window-metal — vendored, one-line patch

Upstream 1.1.0 (the latest release), with a single change in
`src/observer.rs`: the `#[name = "RawWindowMetalLayer"]` attribute on
`ObserverLayer` is removed so `objc2` auto-generates the Objective-C runtime
name.

## Why

An explicit `#[name]` must be unique across the whole **process**. Each FTS
plugin bundle statically links its own copy of this crate, so the second
plugin to create a Metal surface tried to register a class the first one had
already registered. `objc2` panics with `class_not_unique`, the panic unwinds
through an AppKit block, and the host aborts:

```
9: objc2::__macro_helpers::define_class::class_not_unique
12: raw_window_metal::observer::ObserverLayer::new
14: <wgpu_hal::metal::Instance as wgpu_hal::Instance>::create_surface
20: nice_plug_dioxus::wgpu_state::WgpuState::new_from_raw
libc++abi: terminating due to uncaught foreign exception
```

Loading any two FTS plugins into one REAPER was enough. That is not a corner
case for this product — the suite exists to be used together.

## Why patched rather than upgraded

1.1.0 is the newest published version, so there is nothing to bump to.

## Why this specific fix

It is what `objc2` documents. From `define_class!`:

> The name must be unique across the entire application. If you're developing
> a library, it is recommended that you do not set this ... If the name is
> auto-generated, the class will also be allowed to be used across multiple
> shared dynamic libraries in the same process.

Worth sending upstream: any library used from more than one dylib hits this,
so the fix belongs in the crate rather than in every consumer's vendor dir.

Licence unchanged (MIT OR Apache-2.0) — see `LICENSE-MIT` / `LICENSE-APACHE`.
