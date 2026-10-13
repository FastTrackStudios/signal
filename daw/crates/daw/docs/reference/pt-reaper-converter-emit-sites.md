# PT Reaper Converter — RPP emit-site catalog

ARM64 instruction addresses where the converter emits each RPP keyword,
discovered by scanning MOV/MOVK immediate chains for ASCII strings.

The function containing offsets `0x56xxx`–`0x5axxx` and `0x60xxx`–`0x66xxx`
is the master per-track RPP serializer. Each emit site is the MOV that
loads the keyword string before a format call.

## Emit-site addresses (ARM64 slice, load base = 0x100000000)

| RPP keyword | Sites | First load addr | Notes |
|---|---|---|---|
| `MUTESOLO`   | 4  | `0x100056b94` | mute/solo/solo-defeat triple |
| `VOLPAN`     | 12 | `0x100056b20` | per-track volume + pan |
| `MARKER`     | 3  | `0x100053ef0` | timeline markers |
| `PEAKCOL`    | 3  | `0x100057fe0` | track color (REAPER PEAKCOL = `0x01000000 | (B<<16) | (G<<8) | R`) |
| `ISBUS`      | 5  | `0x1000583e4` | folder marker (folder_start / folder_end) |
| `TRACK`      | 8  | `0x100056a78` | per-track block header |
| `TRACKID`    | 8  | `0x100056a78` | track GUID |
| `NCHAN`      | 8  | `0x100056bec` | channel count |
| `MAINSEND`   | 6  | `0x100056ca4` | send to master |
| `AUXRECV`    | 7  | `0x100059698` | track receive (REAPER stores sends as receives on dest track) |
| `FXCHAIN`    | 3  | `0x1000750bc` | plugin chain |
| `TEMPO`      | 7  | `0x100053954` | initial tempo |
| `TEMPOENV`   | 7  | `0x100053954` | tempo envelope points |
| `POSITION`   | 4  | `0x1000571c4` | item position |
| `LENGTH`     | 4  | `0x100057248` | item length |
| `SOFFS`      | 4  | `0x1000573ac` | item slip offset |
| `FADEIN`     | 3  | `0x100065dbc` | fade-in curve |
| `FADEOUT`    | 3  | `0x100065f4c` | fade-out curve |
| `PLAYRATE`   | 3  | `0x1002355b8` | item playback rate |
| `CHANMODE`   | 2  | `0x1000573fc` | mono/stereo conversion |
| `SOURCE`     | 4  | `0x100057558` | media source (`SOURCE WAVE`, `SOURCE MIDI`) |
| `SAMPLERATE` | 3  | `0x1000537e4` | project sample rate |
| `FREEMODE`   | 4  | `0x100056d3c` | track free-positioning items |
| `NOTES`      | 1  | `0x10005aa58` | track notes (item 6 in roadmap!) |

## Discovered mute decision (already documented)

- Function: prologue at `0x100060b28`
- MUTESOLO emit: `0x100061400` (one of the 4 sites)
- Decision: `0x1000612b0`-`0x1000612dc` (checks Swift `Optional<PTXMutePoint>` pointer at `sp+0x88`)

## Methodology for each unknown feature

For each emit site:

1. Walk back ~50 lines in `arm64_disasm.txt` to find which register holds
   the per-feature value at the emit instruction.
2. Frida-hook the emit instruction + dump that register.
3. If the value came from a Swift Optional / class instance, dump the
   struct shape (we did this for mute).
4. Walk further back to find the load-from-source instruction (likely
   `ldr xN, [x?, #offset]`) — that's where the field lives in the
   parsed Swift model.
5. To trace back to the PTX file, Stalker-trace the function that
   populated that struct and find which `pread`/buffer-read provided
   the source bytes.

## Phase B harness — universal emit logger

Live at: `voyager:/tmp/harness_emit.js`

Strategy:
- Hook all emit sites listed above
- Tag each emit with current track name (from a NAME-emit context var)
- Dump x0..x28 at each emit instruction
- Output as JSON lines: `{ts, track, feature, regs}`

Manual post-processing identifies which register held the value at each
site (correlate with known good output values from the captured RPP).
