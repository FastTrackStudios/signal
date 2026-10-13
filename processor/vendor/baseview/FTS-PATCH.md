# baseview — vendored 0.3.3, one-line patch

Upstream 0.3.3 (the newest release) with a single change in
`src/wrappers/appkit/timer.rs`: the frame timer is registered in
`kCFRunLoopCommonModes` rather than `kCFRunLoopDefaultMode`.

## Why

A timer registered only in `kCFRunLoopDefaultMode` stops firing whenever
AppKit runs the run loop modally — which it does for the whole of a live
window resize (`NSEventTrackingRunLoopMode`) and for menu tracking. That
timer drives `on_frame`, so a default-mode registration freezes the window
handler for the entire drag: no relayout, no redraw, and AppKit stretches
the stale surface until the mouse comes up.

Still `kCFRunLoopDefaultMode` upstream at 0.3.3 — verified against
`RustAudio/baseview@master`, so this is carried forward rather than dropped.

## History

This replaces the `FastTrackStudios/baseview` fork pinned at
`fa0c2c870927ec27138f914c0841f305e9b4972f` (0.3.0 + this fix). Upstream had
moved 23 commits and three patch releases past it — 0.3.1, 0.3.2, 0.3.3 —
including `NativeSize` (#329), min/max window sizes (#314), macOS cursor
support (#280, #332), `host_main_thread_callback` no longer requiring `&mut`
(#312), proper standalone mode (#334), and a long run of X11 fixes.

Vendored rather than re-forked so the delta is one reviewable file in this
repo instead of a rev pin in another.

Licence unchanged (MIT OR Apache-2.0).
