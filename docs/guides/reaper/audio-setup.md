---
title: Audio Setup
kind: setup
type: setup
---

# Audio setup

Everything you hear and record runs through one dialog. Get the device, sample rate, and **buffer size** right once and the rest of the guide just works.

- `kbd:@40099` — Open the audio device configuration.

```gif right
audio-device-config
**Pick your interface first.**

- Choose your audio interface as the device (ASIO on Windows, Core Audio on macOS).
- Set the **sample rate** to match your session — 48 kHz is the safe default.
- Everything below lives in this same dialog.
```

## Buffer size — the one knob that matters

Buffer size (block size, in samples) is the tradeoff between **latency** and **CPU headroom**:

- **Small buffer (64–128)** → low latency, tighter monitoring, more CPU load and risk of crackles.
- **Large buffer (512–1024)** → rock-solid playback and heavy plugin counts, but noticeable delay on live input.

The rule of thumb: **small while tracking, large while mixing.**

- **Tracking** a performer who monitors through REAPER? Drop to 64–128 samples so they don't hear themselves late.
- **Mixing** with a full plugin chain? Raise to 512–1024 — nobody's playing live, so latency is free CPU.

At 48 kHz, 128 samples is ~2.7 ms of I/O latency each way; 1024 is ~21 ms. Anything over ~10 ms round-trip starts to feel like a delay to the player.

## Monitoring — hearing yourself without the lag

Even a small buffer adds *some* latency. Two ways around it:

- **Direct/hardware monitoring** — let the interface mix your input to your headphones with zero latency, and turn REAPER's input monitoring off. Best when you don't need to hear input effects.
- **Software monitoring** — monitor through REAPER (so you hear reverb, amp sims, etc.) and keep the buffer small. See [[Recording]] for the monitor toggles.

If you can hear yourself but it feels late, that's buffer size — this page is the fix. See also the [[faq|FAQ]].

Next: [[Transport]] to get playback under your fingers, or jump to [[Recording]] to track.
