// What the rig is hearing, as the remote shows it: one signal everything
// that reacts reads — the meters, and the macros' feedback (a gate's level
// against its threshold, each delay echo, a reverb's tail). The rig streams
// these; here a guitar is simulated — strums on the song's beat, each one
// a burst that fades, with its echoes and its tail — so the reactions can
// be felt. One animation loop for all of it; frozen under reduced motion.

import { useEffect, useState } from "react";

export interface Signal {
  /** Input level, 0–1 (the guitar). */
  input: number;
  /** Output level, 0–1 (the house). */
  output: number;
  /** The phones' level, 0–1. */
  phones: number;
  /** The delay's echoes, 0–1: a pulse on each repeat. */
  echo: number;
  /** The reverb's tail, 0–1. */
  tail: number;
  /** A slow modulation cycle, 0–1. */
  lfo: number;
  /** How hard the compressor is working, 0–1. */
  squash: number;
}

const still: Signal = { input: 0.35, output: 0.4, phones: 0.38, echo: 0, tail: 0.2, lfo: 0.5, squash: 0.1 };
let now: Signal = still;
let bpm = 120;
const listeners = new Set<() => void>();
let raf = 0;
let start = 0;
let onsets: { t: number; amp: number }[] = [];
let nextOnset = 0;
let drawn = 0;

function frame(ms: number) {
  if (!start) start = ms;
  const t = (ms - start) / 1000;
  const beat = 60 / bpm;
  // Strum on the beat (now and then a rest), each a burst that fades.
  if (t >= nextOnset) {
    if (Math.random() > 0.18) onsets.push({ t, amp: 0.55 + Math.random() * 0.4 });
    nextOnset = t + beat * (Math.random() > 0.7 ? 1 : 2);
    onsets = onsets.filter((o) => t - o.t < 6);
  }
  let input = 0;
  let echo = 0;
  let tail = 0;
  const dt = beat * 0.75; // dotted eighths
  for (const o of onsets) {
    const age = t - o.t;
    input = Math.max(input, o.amp * Math.exp(-age / 0.55));
    for (let k = 1; k <= 4; k++) {
      const a = age - k * dt;
      if (a >= 0) echo = Math.max(echo, o.amp * 0.62 ** k * Math.exp(-a / 0.11));
    }
    tail = Math.max(tail, o.amp * 0.85 * Math.exp(-age / 2.1));
  }
  input = Math.min(1, input + Math.random() * 0.04);
  const squash = Math.max(0, Math.min(1, (input - 0.45) * 1.8));
  now = {
    input,
    output: Math.min(1, input * 0.85 + echo * 0.25 + tail * 0.15 + Math.random() * 0.03),
    phones: Math.min(1, input * 0.8 + tail * 0.12 + Math.random() * 0.03),
    echo,
    tail,
    lfo: 0.5 + 0.5 * Math.sin(t * 2 * Math.PI * 0.9),
    squash,
  };
  // Redraw at ~30 fps: enough to feel alive, light on the device.
  if (ms - drawn > 33) {
    drawn = ms;
    for (const l of listeners) l();
  }
  raf = requestAnimationFrame(frame);
}

/** The rig's tempo drives the strums and the echoes. */
export function setSignalTempo(b: number) {
  if (b > 0) bpm = b;
}

/** Read the signal, redrawn every frame while mounted. */
export function useSignal(): Signal {
  const [, tick] = useState(0);
  useEffect(() => {
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduce) return;
    const l = () => tick((n) => (n + 1) % 1e9);
    listeners.add(l);
    if (!raf) raf = requestAnimationFrame(frame);
    return () => {
      listeners.delete(l);
      if (!listeners.size) {
        cancelAnimationFrame(raf);
        raf = 0;
      }
    };
  }, []);
  return now;
}
