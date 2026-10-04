Source: crates.io `winit-uikit` 0.31.0-beta.2 (rust-windowing/winit), the
published tarball with the registry's own bookkeeping files dropped.
License: Apache-2.0 — theirs, not ours, and it stays that way.

Vendored for one change to `src/app_state.rs`:

- **The event-loop proxy's wake-up handles what queued during it.** Events
  that arrive while the handler is busy are queued (re-entrancy guard), and
  every other entry point — `new_events`, `resumed`, `about_to_wait` —
  follows its callback with `handle_nonuser_events(mtm, [])` to drain the
  queue. The proxy wake-up did not. Blitz draws its iOS frames from that
  wake-up (`frame_now`), so a touch that came while a frame was drawn sat in
  the queue until the next event: on the phone, with meters moving and a
  frame in progress most of the time, a tap acted only when the next touch
  arrived — the rail looked frozen. Found by logging pointer events: two
  taps seconds apart reached the page 125 ms apart.

Drop this when upstream drains the queue after `proxy_wake_up`.
