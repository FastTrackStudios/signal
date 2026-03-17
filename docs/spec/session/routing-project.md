# Routing Project

Cross-project audio routing for live performance. Individual song projects send stems
to loopback channels. A singleton Routing Project receives those stems for monitoring/mixing.

## Routing Channels

r[routing.channel.definition]

A `RoutingChannel` enum defines all stem categories:
- Click, Loop, Count, Guide (Click+Guide group)
- Drums, Percussion, Bass, Guitar, Keys, Vocals, SFX (Tracks group)

Each channel provides: display name, group membership, default loopback pair index.

r[routing.channel.groups]

Channels belong to one of two `RoutingGroup` values:
- `ClickGuide` — timing/cue channels (Click, Loop, Count, Guide)
- `Tracks` — instrument stem channels (Drums through SFX)

r[routing.loopback.config]

`LoopbackConfig` maps channels to hardware loopback pairs:
- `base_pair` — 0-based index of first loopback stereo pair
- `pair_index(channel)` = base_pair + channel's default offset
- `recinput_value(channel)` — REAPER `I_RECINPUT` encoding for stereo loopback

## Standalone Routing Project

r[routing.project.ensure]

`ensure_routing_project(config)` resolves in priority order:
1. Scan open projects for ExtState `FTS/is_routing_project == "1"`
2. Check disk at `<fts_home>/Reaper/FTS-Routing.RPP`
3. Create from scratch with canonical track hierarchy

r[routing.project.structure]

The routing project has two folder groups:
- "Click + Guide" folder with Click, Loop, Count, Guide child tracks
- "TRACKS" folder with Drums, Percussion, Bass, Guitar, Keys, Vocals, SFX child tracks

Each child track is configured with:
- 2 channels (stereo)
- Record input set to loopback pair
- Record-armed
- Parent send disabled

r[routing.project.extstate]

Projects are identified via ExtState:
- Section: `FTS`
- Key: `is_routing_project`
- Value: `"1"`
