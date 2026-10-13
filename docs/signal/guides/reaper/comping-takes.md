---
title: Comping & Takes
kind: input
type: input
category: lanes-takes
---

# Comping & takes

Once the take-ranking pass from [[Recording]] has flagged the good moments, comping is about auditioning lanes and stitching the keepers together. The lanes-and-takes layer puts lane playback on a single key and every take operation behind one menu.

## Audition lanes

- `kbd:@42482` — Play only the next lane.
- `kbd:@42481` — Play only the previous lane.

```gif
comping-take-lanes
`t` cycles which fixed lane plays back, so you can flip through takes without soloing by hand.
```

## The take menu (`kbd:<A-t>`)

Press `kbd:<A-t>` — hold Alt and tap letters — for take navigation and operations:

- `kbd:@40125` — Switch items to the next take.
- `kbd:@40126` — Switch items to the previous take.
- `kbd:@40639` — Duplicate the active take.
- `kbd:@40131` — Crop items to the active take (commit the comp).
- `kbd:@40543` — Implode items on the same track into takes.

## Fixed lanes

The lanes submenu handles fixed-item-lane comping — the layout the take-ranking workflow records into:

- `kbd:@42430` — Toggle fixed item lanes on the track.
- `kbd:@41378` — Move the active comp to the top lane.
- `kbd:@42635` — Explode takes onto fixed lanes.

With a comp cropped, move on to shaping the session in [[zoom-views|Zoom & views]] and [[mixing|Mixing]].
