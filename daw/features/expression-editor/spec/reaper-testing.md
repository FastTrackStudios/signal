# Seeing the editor run inside REAPER

The REAPER integration test can be watched and screenshotted, which is
how the panel got verified rather than assumed.

## Why not the built-in modes

- `cargo run -p fts-extensions-xtask` — headless (`DISPLAY=""`). **The
  panel cannot open**: creating a Dioxus window aborts REAPER inside GDK
  (`gdk_cursor_new_from_pixbuf: assertion 'GDK_IS_DISPLAY (display)'
  failed`) and takes the daw socket with it, so every later test in the
  run fails with a socket timeout. Fine for data-only tests.
- `--virtual` — meant to use Xvfb via an `fts-test` launcher that is
  **not installed on this machine**, so it silently falls back to
  headless and behaves identically to the above.

## What works: your own Xvfb, then capture

```sh
Xvfb :99 -screen 0 1920x1200x24 -nolisten tcp &
DISPLAY=:99 cargo run -p fts-extensions-xtask -- --gui
```

`--gui` inherits `DISPLAY` from the environment, so pointing it at a
private Xvfb gets a real display without touching your desktop.

Capturing the **root** window is not enough: REAPER's Actions List sits
over everything, and a bare Xvfb has no window manager, so windows are
unmanaged and stack in whatever order they were mapped. Capture the
panel's own window instead, found by its geometry:

```sh
xwininfo -display :99 -root -tree \
  | grep -oE '0x[0-9a-f]+ .*1180x640' | grep -oE '^0x[0-9a-f]+' \
  | head -1 | xargs -I{} import -display :99 -window {} panel.png
```

Run that in a loop alongside the test — the panel is only up for a
couple of seconds.

Running a lightweight WM on the Xvfb would fix the stacking if
whole-screen shots are ever wanted.

## What the extension log tells you

`~/.local/state/fasttrackstudio/reaper-fts-extensions.log.<date>` is the
fastest check that the panel actually came up:

```
Panel 'FTS_EXPRESSION_EDITOR' client rect w=1180 h=640
Created EmbeddedView for panel 'FTS_EXPRESSION_EDITOR'
Applied floating X11 window hints panel="FTS_EXPRESSION_EDITOR" xid=…
loaded take into editor notes=3
wrote take notes=3
```
