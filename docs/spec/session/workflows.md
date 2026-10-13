# Workflows — everything editing and mixing a project has to be able to do

The master list. A project goes through tracking, comping, editing and
mixing, instrument by instrument, and this is every flow the session
window and the expression editor directly support — each one with a
scene that can be rendered, a gesture that can be driven, and a test
that says it works. When every box here is ticked, the product is
ready. The list is far larger than this; when *this* list is done, we
are in a very good spot.

Tracey prefix `flow`. Each rule is one thing a user does. A rule is
ticked when there is an `r[impl]` in the code and an `r[verify]` in a
test; `tracey query uncovered` lists what is left.

Builds on: `maximal-template.md` (the reference session every scene is
rendered against), `track-organization.md`, `routing-project.md`, and
the expression editor's `drum-mode.md` (`drums.*`) and its alignment
and quantize specs.

## Checklist

| Instrument | Tracking | Comping | Editing | Mixing | More |
|---|---|---|---|---|---|
| Patch List | [ ] the view · [ ] project-level list · [ ] studio profiles · [ ] session override · [ ] apply | | | | |
| Scenes | [ ] REAPER's model underneath · [ ] FTS grouping · [ ] by performer in Record · [ ] rig per performer · [ ] headphone mix · [ ] folder record preview · [ ] Guide folder · [ ] Keyflow folder | | [ ] by arrangement | [ ] by arrangement | [ ] follow the mode · [ ] every scene renders |
| Drums (audio) | [x] full · [ ] overview | [ ] folder items | [ ] edit scene · [x] stack · [x] slip/stretch · [x] quantize · [ ] align hits | [x] scenes · [x] balance | [ ] triggering samples |
| Percussion | [ ] | [ ] on the track | [ ] | [ ] | [ ] folder |
| Bass | [ ] full · [ ] overview · [ ] folder preview | [ ] on the track | [ ] | [ ] later | |
| Guitars (electric and acoustic) | [ ] full · [ ] overview · [ ] grow | [ ] folder items | [ ] edit scene · [ ] doubles aligned | [x] buses/VCA · [ ] layers collapsed · [ ] balance scene · [ ] source defaults | [ ] golden shapes · [ ] same at every depth · [ ] acoustics |
| Vocals | [ ] by performer | [ ] lanes · [ ] parts as folder items | [ ] doubles aligned | [x] lead vocal scenes | [ ] language dimension · [ ] active language · [ ] render per language · [ ] leads · [ ] bgv parts · [ ] choir · [ ] tuning · [ ] automation · [ ] golden |
| Keys | [ ] record MIDI | [ ] | [x] MIDI in the editor | [ ] | [ ] parts |
| Synths | [ ] | [ ] | [ ] | [ ] | [ ] families · [ ] golden |
| Orchestra | [ ] | [ ] | [ ] | [ ] | [ ] sections · [ ] golden · later spec |

## Views a flow is done in

Every flow is looked at through a **scene**: which tracks are shown in
the arrangement's panel (the TCP) and the mixer (the MCP), how large,
and which folders are folded. A scene is a rule resolved against the
session's own taxonomy — never a list of track ids — so it fits a real
session whose shape differs from the reference.

r[flow.scenes.source-track]
A **source track** is a track that records: it has a record input and
is armed, or is the kind of track the session records to (audio input
or MIDI input). Whether a track is a source is read from the track's
record settings, not from its name. A trigger track is a source only
when it is being recorded as audio; a trigger fed by a plugin or a MIDI
conversion is present in the session and is not a source.

r[flow.scenes.per-piece-trigger]
Whether a kit piece's trigger is recorded is decided **per session and
per piece**: a session may record a kick trigger and no snare trigger,
or a snare trigger and no tom triggers. The tracking scene follows that
decision — the trigger is a source track where it is recorded, and only
there. Nothing in the template forces it either way.

r[flow.scenes.reaper-model]
The window is a **control surface over REAPER's data model**. Takes,
fixed lanes and comp areas, track groups, VCAs, ext-state — where
REAPER has a model, the window uses it and never invents a parallel
one, so the same flows operate on REAPER directly and on
daw-standalone, which implements the same model. A stereo pair — OH,
Rooms, Piano: two mics capturing one thing — is one stereo track; a
double is two performances and is two mono channel tracks, never a
stereo track.

r[flow.scenes.groups]
REAPER's track groups are a **finite resource shared across every
project of an album**, and the flows that need tracks to move
together — arm follows the layer, the fader follows the folder, mute
follows the language — are provided by **FTS's own grouping watcher**
in the dynamic template wherever the facade allows, spending no
REAPER group. Where a REAPER group is still needed (a VCA lead), it is
allocated **from the top of the range downward**, consistently across
the album, leaving the front of the range free for the session's own
use.

r[flow.scenes.render]
Every scene named in this spec renders headless from the reference
session (`just daw-scene <slug>`) and the render is a test fixture: a
change to what a scene shows is a change to a picture, not a surprise.

r[flow.scenes.follow-mode]
Scenes are the **visibility manager**, and the visibility manager
follows the DAW mode: entering **Record** shows the instrument's
tracking scene, **Edit** its editing scene, **Mix** its mixing scene,
without a second choice being made. A mode has a default scene per
instrument, the number keys still recall any scene inside a mode, and
a scene chosen by hand stays until the mode changes. Comping is not a
mode; its view lives inside Record and Edit (see
`flow.drums.comping.folder-items`), because a kit is comped while it
is still being tracked as often as afterwards.

r[flow.scenes.folder-record-preview]
A **folder track shows the recording as it happens** even while it is
closed: the waveform of what is being recorded to the tracks beneath
it draws on the folder's row, live, as a folder item of the take in
progress. That is what lets the bass folder — or a kit's piece, or a
guitar part — stay collapsed in an overview while the take is still
visibly going in. REAPER draws nothing on a closed folder; this does.

r[flow.scenes.performer-order]
Tracking sorts **by performer**; editing and mixing sort **by
arrangement**. In Record mode the rows of an instrument group under
the person playing them — `Guitars / Cody / {Rhythm, Lead, …}`, the
template's Performer level above Arrangement — because while tracking
the things that need doing are about the performer: their input,
their arm, what they hear of themselves. In Edit and Mix the rows
group by arrangement — every rhythm guitar together — and the
performer is not a level. The **project's own folders stay
arrangement-sorted**; the performer grouping is a **view**: a scene
computes its rows from the tracks, so the Record scenes regroup the
same tracks under a performer row without moving a track in the
project, and switching mode switches the grouping.

r[flow.scenes.performer-identity]
Which performer a track belongs to is read from the track's
**Performer** dimension in its name when it has one, and otherwise
from its **headphone bus**: each performer has a headphone bus
(`groups/headphones.rs`, the template's monitor buses), and a track
routed to a performer's headphone mix is that performer's. Assigning a
track to a performer assigns its cue send; the two never disagree
because the send is the assignment. That send is also the **record of
who played what**: the organizer sorts by it, a track can be renamed
into the Performer dimension from it, and "can I have more of myself"
works across the board — for every performer, on every instrument —
because every performer's tracks are already on their bus.

r[flow.scenes.performer-rig]
A performer has a **rig**: the input for each kind of source they
record — the DI, the pedalboard, each amp mic. Every source track of
that performer and that kind **follows the rig**: change the DI input
on Cody's row and every one of Cody's DI tracks takes it, on every
channel of every part; the same for arming and input monitoring. The
performer row is where the rig is set, and it is the one place a
performer's inputs live.

r[flow.scenes.performer-headphones]
The performer row carries their **headphone mix**: the level of their
own tracks in their cue, and the rest of the band in it, so "more of
me" is one control on one row while tracking, in the same view the
rig is set in.

r[flow.scenes.guide-folder]
The **Guide** folder sits at the top of every session and every scene
shows it collapsed to one row unless a flow opens it: **Click**,
**Guide** (the section cues) and **Shaker** — the count and the pulse
the band tracks to. Its tracks go to the headphone mixes and never to
the mix bus. Opening it is a flow of its own, not a side effect of an
instrument's scene.

r[flow.scenes.keyflow-folder]
The **Keyflow** folder sits under the Guide folder: the song's
knowledge as MIDI items — **CHORDS**, **LINES**, **HITS** — and
whatever else keyflow keeps, read by the guide, the expression
editor's key and chord tools, and the click's count. Every scene shows
it collapsed; Write and Produce modes open it.

r[flow.scenes.two-audiences]
A flow has views for two audiences where they differ: the engineer's
full view, with every track the flow touches, and the player's overview
— what the person performing needs to see and nothing else. The
number keys recall scenes in the window; the same scenes are recipes
in the mixer.

## Patch List — the input plan

Where every source track's input comes from is planned in one place,
not set track by track: the **Patch List**, a view of its own in the
session window — the studio's patch list, kept by the software.
The plan is not a property of one session — an album is ten sessions
that have to be tracked the same way — so it lives above the session
and is applied to each, and each session keeps the copy it was
tracked with.

r[flow.patch-list.plan]
The **patch list** names every input the project records and what it
is for: per performer, their rig (`flow.scenes.performer-rig`) —
"Cody's guitar rig is DI 3, pedalboard 4, amp A 57 on 5, amp A 121 on
6 …", "John's guitar rig is …"; per kit, every channel — kick in,
kick out, kick trigger, snare top … — and per bass, its channels. The
plan is edited in the **Patch List** as a table of source kinds
against inputs, grouped the way tracking sorts
(`flow.scenes.performer-order`).

r[flow.patch-list.project-level]
The patch list is saved **with the project — the album — not the session**:
a file beside the sessions that every session of the project applies,
so ten songs are tracked with one setup and a change to the plan
reaches all of them. Each session also stores the patch list it was last
applied with, so a session opened on its own, or years later, still
says where its tracks came from.

r[flow.patch-list.studio-profiles]
A **studio profile** is the room: the physical inputs a location has
and what is patched to them — the patchbay's view of the studio
(`crates/patchbay`). The patch list names inputs by their role in the
profile ("kick in", "DI 3"), and the profile resolves them to the
device's channels, so the same patch list tracks the same album in two
rooms with two profiles and nothing in it changes.

r[flow.patch-list.session-override]
A session can **override** the patch list for the day — a spare mic on a
different channel, a kit tracked in another room, a performer on a
different rig — without editing the list: the override is layered on
top, marked as such in the Patch List, and stays with that session
alone. Removing it returns the session to the list.

r[flow.patch-list.apply]
Applying the patch list sets **every source track's input** from it: a
performer's tracks from their rig by source kind, a kit's channels by
piece, a bass's by channel — and arms and monitoring follow the same
grouping. A track the list has no entry for is shown in the Patch
List as unpatched rather than silently left; an entry with no track
is shown as unused. The apply is one undo step.

## Drums — audio

The kit is the one instrument that can be made perfect once: a kit is
a kit. Every drum flow is rendered against `Drum Kit/` and `Process/`
of the reference session.

### Tracking

r[flow.drums.tracking.full]
**Drum Tracking** shows every source track of the kit in the TCP and
the MCP — every close mic, every overhead and room, and every trigger
that is being recorded — at working size, with the kit's folders open
and everything else in the session collapsed: the Process folder and
the bus tree present at their minimum. This is the engineer's view
while the kit is being dialled in.

r[flow.drums.tracking.overview]
**Drum Tracking Overview** shows the kit as the drummer reads it: one
strip per piece — Kick, Snare, each tom, Hi-Hat, Ride, OH as one
stereo strip, Rooms as one stereo strip — each a folder collapsed to
one row that can be expanded when something needs a look. The maximal
form is

```text
Kick
Snare
Tom 1
Tom 2
Tom 3
Tom 4
Hi-Hat
Ride
OH      (stereo)
Rooms   (stereo)
```

Everything else in the session is collapsed or hidden. This is what
the drummer and the rest of the band look at once the kit is dialled
in and tracking is one part of a whole-band session.

r[flow.drums.tracking.arm]
Arming, input monitoring and input selection are on the strip in both
scenes — the rail's arm on a thin strip, the arm and the monitor lamp
on a working one — and arming a piece's folder arms its sources.

r[flow.drums.tracking.meters]
Every source strip meters its input while armed and its playback
otherwise, at the engine's meter rate, in the TCP and the MCP alike.

### Comping

Twenty takes of a kit are twenty items on the kick, twenty on the
snare, twenty on each tom, and the arrangement becomes impossible to
read. Comping is done on the folder instead.

r[flow.drums.comping.folder-items]
The **Drum Kit folder shows folder items**, the way
[nvk folder items](https://nvk.tools/docs/workflow/folder_items/) do:
one item per take on the folder's row, rendered from the kit's main
sources — the kick, the snare and the toms summed — so a take reads as
one item with one waveform rather than a stack of items across twenty
tracks. The folder item is a **view** of the take's items on the source
tracks underneath; it owns no audio of its own.

r[flow.drums.comping.folder-item-colours]
The folder item's summed waveform is drawn **per piece in the piece's
colour**, not as one grey sum: the kick's contribution in the kick's
hue, the snare's in the snare's, the toms' in the toms' — the role
colours the expression editor's lanes already use (`LaneRole::color`),
so the two views agree. Layered in one item, the kick pattern reads on
its own from across the room, the snare's backbeat sits between it,
and a fill is visibly the toms. That is what makes a take readable as
one item: the pattern, not just the loudness. The same colouring is
used wherever a folder item carries the kit's summed waveform — the
edit scene's rows and the tracking overview's collapsed pieces.

r[flow.drums.comping.folder-lanes]
With the kit in comping, the folder shows **one lane per take**, each
lane the take's folder item, and a **comp lane** on top: the comp is
built by choosing regions of the take lanes, and the comp lane shows
the result as one item. The source tracks are collapsed under the
folder while this is done; expanding a piece shows its own take lanes
for the case a single mic needs a different choice.

r[flow.drums.comping.lanes]
Every take of a source track is a **take lane** under it, shown in the
arrangement when the track is in comping; the kit's take lanes line up
across the pieces because the takes were recorded together.

r[flow.drums.comping.group]
A comp on the kit is made **as a group**: choosing a take's region on
the folder's comp lane chooses the same region on every source track
of the kit, so the kit is never cut between mics. A piece can be taken
out of the group deliberately (a fixed snare hit from another take) on
its own take lanes and put back.

r[flow.drums.comping.crossfade]
Comp boundaries get crossfades of a session-wide default length, drawn
on the items and draggable at the top corners the way any fade is
(`arrangement.rs` fades), and a boundary snaps to a hit's onset when
the kit's hits are detected.

### Editing

r[flow.drums.editing.scene]
**Drum Editing** shows the kit the way the expression editor folds it:
**one row per source piece** — one Kick, one Snare, one row per tom,
one per source track — with the mics under each piece collapsed into
it, the pieces' folder items carrying the piece's summed waveform in
the piece's colour (`flow.drums.comping.folder-item-colours`), and
the Process folder and the buses hidden. It is the scene Edit mode
shows for the kit, and the arrangement's counterpart of the stack's
lanes: the same fold, so what is selected in one is what is edited in
the other.

r[flow.drums.editing.stack]
Editing the kit is done in the expression editor's stack: every mic
folded into the kick, snare, toms and other lanes, the mics' audio
summed per lane, and the detected hits marked on it. The other lane is
heard, not detected — hats, cymbals and rooms carry no hit list and
take no hit gesture. Docked under the arrangement, the stack draws
through the same renderer as the arrangement at the same rate.

r[flow.drums.editing.hands]
A hit is moved by hand: slip, stretch, snap to grid, nudge by the
grid, add and remove — the gestures of `drums.manual.*` — with the
edit written to every mic of the kit as one undo step.

r[flow.drums.editing.quantize]
The kit is quantized as a group with the transient quantizer
(`drums.quantize.*`): a grid, a strength, fills protected, previewed
before it is applied.

r[flow.drums.editing.align-hits]
A take of the kit can be aligned to another performance's hits — the
reference take, a click, a programmed part — by the hit-alignment
engine, so a punched-in section sits on the take around it.

### Mixing

r[flow.drums.mixing.scenes]
Mixing the kit has its scenes: **Drum Mixing** (the pieces as the
instrument, every mic a rail), **Drum Advanced** (the subs, funds,
triggers and verbs open), **Drum FX** (the Process folder's compress
and verb banks), and **Drum Overview** (five faders). Each is a scene
in the window and a recipe in the mixer.

r[flow.drums.mixing.balance]
The parallel compressors are a **balance group**: one fader up brings
the others down by the same amount between them, and a fader at
silence is out of the group until it is raised.

r[flow.drums.mixing.racks]
Every kit track's rack shows what its phase does to it — the tone
chain with its meters, the compressors reducing, the de-essers firing
— live, at the frame rate, from the engine's meters.

### Triggering — adding samples

r[flow.drums.trigger.per-piece]
A sample is added **per piece**: a kick sample on the kick, a snare
sample on the snare, each tom its own, from the piece's detected hits
— never from the audio in the other lane.

r[flow.drums.trigger.from-hits]
The trigger track is rendered from the hit list — one sample per hit
at the hit's velocity — so a hit slipped in the editor moves its
sample with it, and a hit removed removes its sample.

r[flow.drums.trigger.blend]
The sample and the mics are blended on the piece's Sum, with the
sample's level and its polarity against the close mic controls of the
piece, and the blend is what the Drum Mixing scene mixes.

## Percussion

r[flow.percussion.folder]
`Percussion/` is its own folder beside the drums — shakers, tambourine,
congas, claps, whatever the song adds — usually tracked one at a
time, often layered, each instrument a part that comps on the track
like a bass (`flow.bass.comping`) and edits in the stack with itself
as the detection source. It reaches the mix by its own bus under
INST BUS beside DRUM BUS. The golden session carries Shaker,
Tambourine and Claps.

## Bass

Bass is simple: usually one track at a time, sometimes a second for a
layer — a DI and an amp, a sub synth under the bass, an octave. The
one thing that separates it from REAPER is the folder: the Bass folder
stays closed and still shows the take going in
(`flow.scenes.folder-record-preview`).

r[flow.bass.tracking]
**Bass Tracking** shows the bass's source tracks — the DI and the amp,
or the synth's — at working size with everything else collapsed; the
**Bass Tracking Overview** is the Bass folder alone, closed, with the
take's waveform drawing on it live.

r[flow.bass.comping]
The bass comps **on the track directly** — its take lanes under it, a
comp lane on top — and still has a comping scene: **Bass Comping**
opens the bass's tracks with their lanes and collapses the rest. A DI
and amp pair comps as a group.

r[flow.bass.editing]
**Bass Editing** shows the comped bass — one row per layer, sources
folded into it — and the bass is edited in the stack with its DI as
the detection source: its hits moved and quantized like a kit's, the
DI and the amp moving together.

r[flow.bass.mixing]
**Bass Mixing** shows the bass folder and BASS BUS with its rack, the
layers as rails. What a bass mix needs beyond that is defined later.

## Guitars

Guitars are where a session has to be most flexible, and where the
experience has to stay the same however it is set up. A part can be
one DI track, or a double-tracked part with seven sources a side; a
session starts with one guitar and grows a double, a second
arrangement and a harmony as the song is built. The dynamic template's
hierarchy is the vocabulary (`groups/guitars/example-guitar.md`):

```text
Group → Section → Performer → Arrangement → Layers → Channels → MultiMic
```

An **Arrangement** is what the part is for — Clean, Crunch, Drive,
Lead, Rhythm, Chug, "Chorus Lead"; for an acoustic, Strum,
Fingerpick, Nashville. A **Layer** is a voice inside it — Main,
Harmony, an octave. **Channels** are the doubles — L and R, or a
triple. **MultiMic** is the sources of one channel — for an electric,
DI, pedalboard, two amps each with a 57 and a 121: up to seven for a
single take of a single channel, before it is even doubled; for an
acoustic, a DI and two mics. `TrackDimension` in
`track_schema.rs` classifies a track into one of these by its name.

r[flow.guitars.dimensions]
A guitar part is organised by the template's dimensions in that order
— Arrangement, Layer, Channel, MultiMic — and **a level is a folder
only when it has more than one member**. A DI-only double-tracked part
is an L and an R track carrying items, with no folder under them; a
channel with seven sources is a folder over the seven; an arrangement
with a Main and a Harmony is a folder over two layers, each a folder
over its channels. The smallest session and the maximal one are the
same rule at different depths.

r[flow.guitars.grow]
A part grows by session actions, never by restructuring: **double**
a track (add the R channel to a part that had one, folding the
existing track into L), **add an arrangement** (a Chug beside the
Rhythm), **add a layer** (a Harmony beside the Main), **add a source**
(a second amp mic to every channel of a part). Each action inserts
the missing level around what exists, names the new tracks so the
classifier reads them back into the same dimensions, and leaves items
and routing where they were.

r[flow.guitars.tracking.full]
**Guitar Tracking** shows every source of the part being recorded —
each channel's DI, pedalboard and amp mics — at working size with
their arm, monitor and input on the strip, the other parts' folders
collapsed, the electrics and the acoustics as separate folders. This
is the engineer's view while a part is dialled in.

r[flow.guitars.tracking.overview]
**Guitar Tracking Overview** is the structure set up once and then
collapsed to **one folder per part**: a double-tracked part is one row
whose folder item is the double rendered as one stereo waveform, L and
R as the two sides; a triple is that row with its channels stacked
under it when opened; each channel can carry the full stack of
sources without the overview showing any of it. A DI-only part with
no folder shows as its own tracks. This is what the player looks at.

r[flow.guitars.folder-items]
A part's folder item sums its channels, a channel's folder item sums
its sources, and a layer's folder item sums its channels — the same
folder-item rule as the kit (`flow.drums.comping.folder-items`) at
every level. A double's folder item draws L and R as the two sides of
one stereo waveform, each side in its own shade, so a double reads as
two performances in one item.

r[flow.guitars.comping]
A part comps on its folder as the kit does: one lane per take on the
part's row, each take one folder item however many channels and
sources it has, a comp lane on top, and a choice on the comp lane
made on every source of every channel. A channel or a source can be
opened for its own choice and closed again.

r[flow.guitars.editing.scene]
**Guitar Editing** shows one row per part with its channels as its
lanes — the double as L and R, the harmony under the main — and every
source folded into its channel, so an edit to a channel is an edit to
its seven sources at once.

r[flow.guitars.editing.doubles]
Doubled parts are **aligned**: the second take of a rhythm part is
retimed to the first by the alignment engine (`align.rs`), with a
reference and a dub chosen by the user, the alignment previewed as
warp markers, and applied as one undo step across every source of the
dub's channel. This is the whole alignment system applied to guitars;
the editor's stack shows both channels on one timeline while it is
done.

r[flow.guitars.mixing]
**Guitar Mixing** shows the electric and acoustic folders, their buses
(GTR RHYTHM, GTR LEAD, GTR SOLO under ELECTRIC BUS; ACOUSTIC BUS) and
the Inst FX returns, with the folder's fader as the VCA lead of its
bus. By default **every layer and every multi-mic folder is
collapsed**: almost all of a guitar mix happens at the arrangement —
the part — and its bus, so that is the level the scene shows.

r[flow.guitars.mixing.balance-scene]
**Guitar Balance** is the scene that opens what Guitar Mixing hides:
every channel's sources as strips, so the initial balance and panning
of a configuration can be set. It is the one guitar scene that shows
the multi-mic level at working size.

r[flow.guitars.mixing.source-defaults]
A configuration's sources start with a **default balance**, applied
when a part is created or a source is added, and it is what the
golden session is laid out with:

- A **DI beside any other source is muted and centred** — it is the
  reamp and the safety, not the sound.
- A **pedalboard beside an amp is muted**: the amp, the "Main", takes
  priority.
- **One amp with a 57 and a 121**: the two mics panned **hard left and
  hard right**.
- **Two amps, each with a 57 and a 121**: the mics stay **hard left
  and hard right** inside each amp, and the **amp's own track is
  panned 60 % to a side** — Amp 1 60 % left, Amp 2 60 % right — so
  both mics of both amps are heard, each amp leaning to its side.

A source a user has moved keeps its place; the default applies to
what has never been set.

r[flow.guitars.acoustics]
Acoustics are the same structure with their own sources. A layer's
channel is recorded as a **DI and two mics** — a pickup DI, a
neck-side condenser, a body-side condenser, or whatever the pair is
that day — and then doubled, tripled, harmonised and layered exactly
as an electric part is: Arrangement (Strum, Fingerpick, Nashville),
Layer (Main, Harmony), Channel (L, R), MultiMic (DI, Neck, Body). The
acoustic folder's parts reach ACOUSTIC BUS, and every rule in this
section — the dimensions, growing a part, the overview, folder items,
comping, the edit scene, alignment — applies to an acoustic part
unchanged. A Nashville-strung layer over the steel is a Layer of the
same part, so it comps and aligns with it.

r[flow.guitars.golden]
The golden session (`maximal-template.md`) carries every guitar shape
the rules have to handle, so each scene renders against all of them:

- **Rhythm** — double-tracked, each channel with the seven sources
  (DI, pedalboard, two amps with a 57 and a 121 each), and two
  **octave layers** doing the same part, layered together: the
  arrangement is a folder over Main and Octave, each a folder over L
  and R, each channel a folder over its seven.
- **Lead** — a double-tracked **DI-only** part: L and R tracks carrying
  items, no folder under them.
- **Solo** — a **single DI track**, and a **Harmony Solo** beside it: an
  arrangement with a Main layer and a Harmony layer of one track each.

r[flow.guitars.same-everywhere]
Every guitar scene and gesture works at every depth of the hierarchy:
the overview, the comp, the edit scene and the alignment are the same
on a one-track DI part and on a triple-tracked part with seven sources
a channel. A configuration the rules above do not handle is a bug in
the rules, not a special case for the session.

## Vocals

Vocals are the most complicated instrument in the session, and the
golden session carries the maximal case: a song in **three
languages** — English, Spanish, Portuguese — with **two lead
soloists**, background vocals in every part a vocal arrangement uses,
and a choir. The template's dimensions hold (Performer → Arrangement
→ Layers → Channels → MultiMic), with one more on top.

r[flow.vocals.language]
**Language** is a dimension of every vocal source track: a track is
sung in one language, or in **All** when it is language-free — a
wordless "Hey!", a hummed pad. It is read from the track's name or
inherited from a Language folder. Language sits **under the
performer and the layer, at the source level**, never above them:
`Vocals / Ron / Main / {EN, ES, PT}` and `Vocals / Ron / DBL / {EN,
ES, PT}`. The `Ron` track is where Ron's vocal is **mixed** — one
chain of plugins, one set of sends, for every language — and the
`DBL` track under it is where his doubles are mixed, the same way;
the language tracks under them are sources only, carrying takes and
the corrections a take needs (its tuning, its alignment, its comp).
The English and the Spanish vocal get the same mix because they go
through the same track, and one chain per performer instead of one
per language is also most of the CPU. A performer who does not sing a
language simply has no source track in it — Aline sings only the
Portuguese version, Belen does not sing on it.

r[flow.vocals.language.active]
The session has an **active language**, and every vocal scene follows
it: tracking, comping, editing and mixing show that language's
source tracks, hide the other languages' sources, and the other
languages are **muted through their VCA** — one VCA per language,
`VOX EN`, `VOX ES`, `VOX PT`, over every source track of that
language across every performer, part and choir section, with `All`
always audible. The mix tracks above them — `Ron`, `DBL`, a BGV part
— are the same tracks whatever the language and stay in view.
Switching the language switches the scene and the VCAs in one step,
so "look at the English version" is one action and it cannot leave a
Spanish double audible.

r[flow.vocals.language.render]
Rendering the project renders **one version per language**: each
render with that language's VCA up and the others muted, the
language-free tracks in every one, named for the language. The
active language does not have to be changed to render the others.

r[flow.vocals.leads]
A **lead** is a performer — the golden session has two soloists — and
each lead is a **mix track** with a **Main** and a **DBL** under it,
each of those a layer holding one source track per language the
performer sings. The performer's mix track carries the chain and the
sends; the DBL track carries the doubles' chain; the sources carry
the takes. The lead row of an overview is the soloist, collapsed,
its folder item the active language's Main with the DBL under it.

r[flow.vocals.bgvs]
**Background vocals** are arranged by **part** — Octave Down, Octave
Up, Higher Harmony, Lower Harmony, Whisper, Bass, Tenor, Alto,
Soprano — each part a mix track like a lead's, usually doubled, with
its layers' source tracks per language under it. A part may carry
**as many layers as it needs**: a "Hey!" of fifty layers is fifty
source tracks under one part, mixed on the part, and the part's row
shows them as one folder item summed. Nothing in the scenes, the comp
or the edit assumes a count.

r[flow.vocals.choir]
A **choir** is a folder beside the BGVs — its sections (Soprano, Alto,
Tenor, Bass) as parts, each a mix track layered like a BGV part with
its sources per language under it — on the language's VCA like
everything else, collapsed to one row per section in every overview.

r[flow.vocals.tracking]
**Vocal Tracking** shows the active language's leads and every BGV
and choir part being recorded, sorted by performer
(`flow.scenes.performer-order`) — Ron's tracks together, Belen's
together — at working size with their rig on the performer row, the
vocal FX returns present for the headphone mix, and every other
language hidden and muted.

r[flow.vocals.comping]
Every vocal source track comps on its take lanes with a comp lane on
top, one take chosen per phrase; a lead's Main and DBL comp on their
own lanes; a part's layers comp on the part's folder as the kit does,
one lane per take of the whole part, so a fifty-layer "Hey!" is comped
once. Crossfades snap to phrase boundaries. A comp — and a tuning, and
an alignment — is **per language source**: the English and the Spanish
vocal are different recordings, and nothing done to one is applied
to the other.

r[flow.vocals.editing.doubles]
Doubles, harmonies and layers are **aligned** to the lead: reference
the lead's Main, dub the double or the part, retime the dub, previewed
and applied as one undo step across every layer of the part — the
same engine as the guitars, on vocal features.

r[flow.vocals.tuning]
A vocal is **tuned** in the expression editor: the take analysed to
pitch, drawn as blobs with pitch curves, corrected by hand or snapped
to the key, and written back as the retuned audio — docked under the
arrangement like the kit's stack, on the active language's tracks.

r[flow.vocals.mixing.main]
**Lead Vocal** shows each soloist's mix track and DBL track — where
the vocal is mixed, whatever the language — with the active
language's sources under them, the room and verb returns, the racks,
and the lead vocal bus; the BGVs and the choir collapsed to their
parts.

r[flow.vocals.mixing.fx]
**Lead Vocal FX Edit** opens the vocal's returns — delays, verbs,
widener, pitch — as wide focused strips with their racks, so an effect
is dialled in with its display in front of the engineer.

r[flow.vocals.mixing.automation]
The vocal's level, sends and effect parameters are automated in the
arrangement: envelopes drawn on the track's lanes, points dragged,
written from a fader move, and shown per parameter.

r[flow.vocals.golden]
The golden session's vocals: **EN, ES and PT**; leads **Ron** (all
three), **Belen** (EN and ES) and **Aline** (PT only), each a mix
track with Main and DBL under it and a source per language under
those; BGVs with every part above, doubled, and one
"Hey!" of many layers in `All`; a four-section choir per language;
`VOX EN`, `VOX ES`, `VOX PT` VCAs; and a render per language.

## Orchestra

The orchestra is in the golden session so every scene has to cope
with its shape, and it will get a spec of its own later; for now the
sections and their instruments, sorted the way the template already
sorts them (`groups/orchestra`, `groups/strings`, `groups/horns`).

r[flow.orchestra.sections]
`Orchestra/` holds four sections — **Winds, Brass, Strings, Orch
Percussion** — each a folder with its instruments as parts, each part
layered, doubled and multi-miked by the same dimensions as a guitar
part (`flow.guitars.dimensions`), so every scene and gesture in this
spec applies to an orchestral part unchanged. The section is the
mixing level; the instrument is the tracking and editing level.

r[flow.orchestra.golden]
The golden session's orchestra: **Winds** — Flute, Oboe, Clarinet,
Bassoon; **Brass** — Trumpets, Horns, Trombones, Bass Trombone, Tuba;
**Strings** — Violin 1, Violin 2, Viola, Cello, Bass; **Orch
Percussion** — present and empty for now. Each section reaches the
mix by its own bus under INST BUS. The orchestral flows — divisi,
section balancing, the seating and spot mics, scoring against
picture — are expanded in a later spec; nothing here forecloses them.

## Keys

r[flow.keys.parts]
`Keys/` holds the keyboards as parts — the golden session's are
**Piano, Rhodes, Wurli, Organ** — each a track or, where it was
captured in several ways (a Rhodes DI and its amp, a piano's close
pair and room), a part folded by the same dimensions as a guitar part.
Piano is a stereo track. Keys reach the mix by KEYS BUS.

r[flow.keys.midi-editing]
A MIDI part is edited in the expression editor: the roll and the
velocity strip, notes drawn, moved, trimmed and deleted, velocity set,
the grid and snap from the session's tempo, docked under the
arrangement.

r[flow.keys.recording]
A MIDI part is recorded to an armed track from a MIDI input, with the
count-in and the click from the session, and the take appears in the
arrangement as it is played.

r[flow.keys.comping]
MIDI takes comp on take lanes like audio takes, and a comp of a MIDI
part is one MIDI item.

## Synths

r[flow.synths.families]
`Synths/` is sorted by **family**, the template's own (`groups/synths`,
prefix `SY`): **SY Arps, SY Pads, SY Leads, SY Chords**, each a folder
over the synth tracks of that kind, with the **general synth tracks**
that belong to no family beside them at the top of `Synths/`. A
family is the mixing level; a synth is the tracking and editing
level, MIDI or audio, and a synth that was printed keeps its MIDI
under it as a layer.

r[flow.synths.golden]
The golden session's synths: `Synths / {SY Arps / Arp, SY Pads / Pad,
SY Leads / Lead Synth, SY Chords / Chord}` and two general synth
tracks beside them, all to KEYS BUS with the keys, in the lime the
template gives synths.

## What proves it

r[flow.verify.scenarios]
Every flow above has at least one scenario test that drives the real
gesture — a press, a drag, a key through the profile — against a
project and reads the result back from the project, not from the
picture.

r[flow.verify.frame-rate]
The studio benchmark — a 5120x1440 arrangement with the editor docked
and a 2560x1440 mixer, both every frame — measures the views these
flows are done in against 240 Hz, and a change that costs a frame
shows in it before it ships.
