# Reference implementations — what we read, and what we may use

Two REAPER projects worth reading for the audio editor. Their licences
differ, and the difference decides what we are allowed to do with each,
so it is stated first.

Both are cloned to `.reference/` (gitignored) by:

```bash
mkdir -p .reference && cd .reference
git clone --depth 1 https://github.com/b451c/SneakPeak.git
git clone --depth 1 https://github.com/MichaelPilyavskiy/ReaScripts.git mpl-reascripts
git clone --depth 1 https://github.com/ReaTeam/ReaScripts.git 80icio
```

| project | licence | what that permits |
|---|---|---|
| [SneakPeak](https://github.com/b451c/SneakPeak) | **MIT** (b451c, 2025–2026) | port code, with attribution |
| [mpl ReaScripts](https://github.com/MichaelPilyavskiy/ReaScripts) | **no LICENSE file** | read only — clean-room |
| [Perfect Timing](https://github.com/ReaTeam/ReaScripts) (80icio) | **no LICENSE file** | read only — clean-room |

A repository with no licence is all rights reserved, whatever its
visibility. mpl's scripts get the same treatment MPElodyne already gets
in this tree: read the algorithm, describe it here, design our own. No
lines cross over.

ReaTeam/ReaScripts — where Perfect Timing lives — is a community
collection, and neither the repository nor the script carries a licence
grant. Same rule.

## SneakPeak — the REAPER audio API, done properly

A native C++17 extension, ~31k lines: a dockable waveform item editor
with dynamics, spectral view, multi-item layering and metering. It is
the closest thing to what we are building that exists for REAPER, and
it is doing the same API dance our `AudioSession` does.

### Two things it does that we were not

Both found by reading `waveform_view.cpp::LoadConcatenated`, and both
already fixed:

1. **Item volume has to be applied by hand.** A take audio accessor
   returns *source* audio; REAPER applies item and take gain at
   playback. SneakPeak multiplies by `D_VOL` on the item and again on
   the take after reading.

   This matters for more than looks. Our silence floor — the threshold
   separating a consonant from a gap between phrases — is absolute, so
   an item with the fader down would have had every frame below it and
   produced no sibilant spans at all.

   We apply the item's gain. Take volume is *available* — `Take` in
   the proto carries `volume` and `Takes::get_takes` returns it, so an
   earlier note here claiming otherwise was wrong — but is not applied
   yet, because `AudioSession::load` takes the gain as a parameter and
   only the item's is threaded through. A small follow-up.

2. **Chunk size.** 64 k frames per `GetAudioAccessorSamples` call, which
   we now match. There is no reason to differ from a number already
   proven against real takes.

### Where it gets its format from

SneakPeak asks the `PCM_source` directly — `GetSampleRate()`,
`GetNumChannels()` — rather than probing the accessor, and clamps
channels to two. Our `read_all` probes with a one-sample read instead,
because the facade exposes the accessor and not the source.

The probe is now a real part of the contract rather than a guess: a
zero `sample_rate` or `num_channels` in a `GetSamplesRequest` means
"tell me what you have". That matters because *naming* a rate makes a
host resample, and edits made against a resampled take land in the
wrong place.

REAPER honours it the way SneakPeak gets its format — by asking the
`PCM_source`, captured when the accessor is created, because the
accessor genuinely cannot be asked. Until the REAPER take-envelope
tests went in, only the standalone impl honoured it and the REAPER one
echoed the zero straight back.

Those tests also caught the reason a real take read as almost nothing:
**`GetAudioAccessorSamples` returns 0 or 1, not a sample count.** The
backend was treating the 1 as "one frame" and truncating every read to
a single sample per channel, which presents as a take containing no
audio rather than as a bug. A facade method for source format would
still be tidier than the probe.

### Worth reading before building the chrome

- `mpl_Peak follower tools.lua` gave us the one thing no header
  documents: **there is no API to create a take envelope.** You bring
  one into existence by running the action a user would — `40693` for
  volume, `41612` for pitch, both verified in the wild — on the
  *selected* item, then re-enumerating to find it. The pan and mute
  ids are not attested anywhere we read, so the backend declines rather
  than guessing an id that would silently do something else.
- `waveform_rendering.cpp` — peak + RMS at a dB scale with a
  zero-crossing line. Our backdrop is a plain peak envelope; theirs is
  what a mastering engineer expects to see.
- `spectral_view.cpp` — async FFT spectrogram with channel-pair packing
  (their README claims ~10× on stereo). If the audio editor ever grows
  a spectral view, start here rather than from a textbook.
- `minimap_view.cpp` — the overview strip the manual calls for, already
  solved against the same API.
- `marker_manager.cpp` — REAPER marker/region round-tripping.
- `loop_finder.cpp`, `deess_engine.cpp`, `spectral_repair.cpp` —
  adjacent features, all MIT, all portable if wanted.

## mpl — Align Takes

`Various/mpl_Align takes.lua`, and the forum thread that documents it
([t=179544](https://forum.cockos.com/showthread.php?t=179544)). It is
the free native answer to VocAlign / RevoicePro: pick a **reference**
take, pick a **dub**, and the dub is retimed to match by writing
**stretch markers** into it.

The shape of it, from the thread and the script's own UI:

1. *Get reference* — analyse the reference item's timing.
2. *Get dub* — analyse the item(s) to be matched.
3. Correspondences between the two are turned into stretch markers on
   the dub, so the retiming is non-destructive and stays editable in
   REAPER afterwards.

**Clean-room note.** The above is behaviour, taken from the thread and
the UI. The matching itself we design ourselves — and we are well
placed to, because we already have per-frame f0, RMS and unvoiced spans
from `analyze_take`, which is a richer feature set than a script
working through ReaScript can cheaply get.

### How alignment fits what we already have

Aligning two performances is a correspondence problem between two
analyses, and both halves already exist:

- `Analysis` gives per-frame pitch, level and voiced/unvoiced state for
  a take — that *is* the feature vector to align on. Onsets from the
  unvoiced→voiced transitions are the strongest cue in a vocal, and we
  already compute them.
- The result is a time map: source time → target time. We have one of
  those, `WarpMarker`, and a renderer that honours it
  (`render_world_warped`). So an alignment produces exactly the same
  artefact a timing edit does, and needs no new write path.

That is the whole reason this belongs in this editor rather than beside
it: alignment is the timing feature with its target computed from
another take instead of from a drag.

### What we built

`align.rs` — banded DTW over per-frame features. Three choices carry
most of the quality, and each was forced by a test:

- **Energy is the primary cue, pitch a weak one.** A stacked harmony is
  deliberately at a different pitch and must still align on timing; a
  test aligns a third to its lead to keep that true.
- **Silence pairs at zero cost**, so slack collects between phrases and
  the phrases keep their own rhythm. A long breath in one take then
  cannot drag the whole alignment.
- **A tiebreak toward not moving.** Silence being free means the path
  through a gap is otherwise arbitrary and the map wanders. The bias is
  measured against *identity*, not against the length-scaled search
  centre — biasing toward the centre argues for spreading a length
  difference evenly across the take, when the truth is usually that the
  dub simply came in late and the correction belongs at the front.

`max_shift_secs` is enforced on the finished map, not just as a search
band. The band is around the length-scaled diagonal so takes of
different length can match at all, and a shift inside that band can
still exceed what the user allowed. Better a partly-corrected take than
one silently dragged five times further than asked.

## Perfect Timing — grid quantize

`Items Editing/80icio_Perfect Timing! - Audio Quantizer.lua`, and the
[forum thread](https://forum.cockos.com/showthread.php?t=288964). Beat
Detective for REAPER: detect transients, snap them to the grid, either
by splitting and sliding or by warping.

Read for two things, both of which changed our design and are written up
in `grid-quantize.md`:

- **The detector is a dual envelope-follower gate**, not spectral flux.
  Sample-accurate by construction, where an STFT is quantised to its hop
  — which is 5.8 ms at our settings and therefore the same size as the
  timing error being corrected.
- **The crest test is a trigger condition**, not a post-hoc measurement:
  the ratio between a fast and a slow envelope follower *is* how struck
  a sound is, and it is available at the instant the hit arrives.

Its grid rule — each division claims at most one transient, the loudest
in a window either side — we had already arrived at independently for
the same reason (a buzz roll must not put eight hits on one beat), which
is some evidence it is the natural answer rather than a borrowed one.
