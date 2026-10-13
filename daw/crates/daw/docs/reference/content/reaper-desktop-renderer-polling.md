# REAPER Desktop Renderer Polling

The Linux `fts-ui-desktop` path uses an embedded Dioxus desktop webview inside a
SWELL dock panel. REAPER's normal extension timer is too coarse for smooth
webview work, so each visible desktop-rendered panel starts a per-panel SWELL
`WM_TIMER` at 16 ms after the webview is created.

The fast timer only drives an already-created desktop webview. Native Blitz
panels keep using the normal REAPER timer. The fast timer is stopped whenever
the panel is hidden through `DockWindowRemove`, when SWELL sends
`WM_SHOWWINDOW(false)`, and during `WM_DESTROY`.

## Cadence Logging

Every visible desktop-rendered panel reports a five-second rolling cadence
summary:

```text
Desktop renderer poll cadence panel=<id> source=swell_timer polls=<n> avg_ms=<n> max_gap_ms=<n>
```

`source=swell_timer` means the panel is being driven by the per-panel SWELL
timer. `source=reaper_timer` means the normal extension update loop also reached
the desktop view. In steady state on Linux desktop renderer builds, the SWELL
timer should dominate and the average should be close to 16 ms while the panel
is visible.

For open/close/dock/undock checks, also watch the paired lifecycle logs:

```text
Started SWELL fast timer for desktop Dioxus panel panel=<id> reason=<create|show>
Stopped SWELL fast timer for desktop Dioxus panel panel=<id> reason=<hide|showwindow_hide|destroy>
```

The expected result is one active fast timer per visible desktop-rendered panel,
no cadence logs after hide or destroy, and no reentrant update warnings.
