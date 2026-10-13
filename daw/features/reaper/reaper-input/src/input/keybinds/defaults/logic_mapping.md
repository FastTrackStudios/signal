# Logic Pro to REAPER Action Mapping

This document maps Logic Pro keyboard shortcuts to equivalent REAPER actions.

## Key Notation
- `<D-x>` = Command (⌘) + x
- `<M-x>` = Option (⌥) + x
- `<S-x>` = Shift + x
- `<C-x>` = Control + x
- `<space>` = Spacebar
- `<enter>` = Enter/Return

## Transport Controls

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Play/Stop | Space | 40044 | Transport: Play/stop |
| Play | Numpad Enter | 1007 | Transport: Play |
| Stop | Numpad 0 | 1016 | Transport: Stop |
| Pause | Numpad . | 1008 | Transport: Pause |
| Record | R | 1013 | Transport: Record |
| Record Toggle | Numpad * | 40046 | Transport: Toggle record |
| Rewind | , | 40084 | Transport: Rewind a little bit |
| Forward | . | 40085 | Transport: Fast forward a little bit |
| Go to Beginning | Return | 40042 | Transport: Go to start of project |
| Go to Position | / | 40222 | View: Go to next measure |
| Toggle Repeat/Cycle | C | 1068 | Transport: Toggle repeat |
| Toggle Metronome | K | 40364 | Options: Toggle metronome |
| Toggle Count-in | Shift+K | 40265 | Options: Toggle count-in before recording |

## Markers & Navigation

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Go to Previous Marker | ; or , | 40172 | Markers: Go to previous marker/project start |
| Go to Next Marker | ' or . | 40173 | Markers: Go to next marker/project end |
| Insert Marker | Option+' | 40157 | Markers: Insert marker at current position |
| Rename Marker | Shift+' | 40171 | Markers: Edit/name marker near cursor |
| Delete Marker | Option+Delete | 40613 | Markers: Delete marker near cursor |
| Go to Marker 1-10 | Numpad 1-0 | 40161-40170 | Markers: Go to marker 01-10 |

## Loop/Locators

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Set Loop Start | | 40222 | Loop points: Set start point |
| Set Loop End | | 40223 | Loop points: Set end point |
| Go to Loop Start | | 40632 | Go to start of loop |
| Go to Loop End | | 40633 | Go to end of loop |
| Set Locators by Selection | U / Cmd+U | 40290 | Time selection: Set time selection to items |
| Move Loop Forward | Shift+Cmd+. | 40039 | Move loop points up (later) |
| Move Loop Backward | Shift+Cmd+, | 40038 | Move loop points down (earlier) |

## Track Selection

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Select Previous Track | Up Arrow | 40286 | Track: Go to previous track |
| Select Next Track | Down Arrow | 40285 | Track: Go to next track |
| Toggle Track Mute | M | 40281 | Track: Mute/unmute tracks |
| Toggle Track Solo | S | 40280 | Track: Solo/unsolo tracks |
| Toggle Input Monitoring | Ctrl+I | 40495 | Track: Toggle track input monitor |
| Arm for Recording | Shift+R | 40490 | Track: Arm all tracks for recording |

## Editing

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Split at Playhead | T / Cmd+T | 40757 | Item: Split items at edit cursor |
| Delete Selected | Delete | 40006 | Item: Remove items |
| Cut | Cmd+X | 40059 | Edit: Cut items/tracks/envelope points |
| Copy | Cmd+C | 40060 | Edit: Copy items/tracks/envelope points |
| Paste | Cmd+V | 40058 | Edit: Paste items/tracks |
| Undo | Cmd+Z | 40029 | Edit: Undo |
| Redo | Shift+Cmd+Z | 40030 | Edit: Redo |
| Select All | Cmd+A | 40182 | Item: Select all items |
| Duplicate | Cmd+D | 41295 | Item: Duplicate items |

## Views & Windows

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Open Mixer | X / Cmd+2 | 40078 | View: Toggle mixer visible |
| Open Piano Roll | P / Cmd+4 | 40153 | Item: Open in built-in MIDI editor |
| Toggle Library/Browser | Y / O | 40271 | View: Show media explorer |
| Show/Hide All Plugin Windows | V | 40549 | FX: Show/hide all floating FX windows |
| Full Screen | Ctrl+Cmd+F | 40346 | View: Toggle fullscreen |
| Close Window | Cmd+W | 40031 | File: Close current project tab |

## Automation

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Show/Hide Volume Envelope | Ctrl+Option+V | 40406 | View: Show volume envelope for all tracks |
| Show/Hide Pan Envelope | Ctrl+Option+P | 40407 | View: Show pan envelope for all tracks |
| Toggle Automation Read | Ctrl+Cmd+R | 40878 | Global automation: Set all tracks to automation mode read |
| Toggle Automation Touch | Ctrl+Cmd+T | 40879 | Global automation: Set all tracks to automation mode touch |
| Toggle Automation Latch | Ctrl+Cmd+L | 40880 | Global automation: Set all tracks to automation mode latch |

## Zoom

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Zoom In Horizontal | Cmd+Right | 1012 | View: Zoom in horizontal |
| Zoom Out Horizontal | Cmd+Left | 1011 | View: Zoom out horizontal |
| Zoom In Vertical | Cmd+Up | 40111 | View: Zoom in vertical |
| Zoom Out Vertical | Cmd+Down | 40112 | View: Zoom out vertical |
| Zoom to Fit | Z | 40295 | View: Zoom to fit project in arrange |
| Zoom to Selection | Option+Z | 40031 | View: Zoom time selection |

## Screensets (recall with numpad)

| Logic Function | Logic Key | REAPER Action ID | REAPER Action Name |
|---------------|-----------|------------------|-------------------|
| Screenset 1 | Numpad 1 | 40454 | Screenset: Load window set #1 |
| Screenset 2 | Numpad 2 | 40455 | Screenset: Load window set #2 |
| Screenset 3 | Numpad 3 | 40456 | Screenset: Load window set #3 |
| Screenset 4 | Numpad 4 | 40457 | Screenset: Load window set #4 |
| Screenset 5 | Numpad 5 | 40458 | Screenset: Load window set #5 |
| Screenset 6 | Numpad 6 | 40459 | Screenset: Load window set #6 |
| Screenset 7 | Numpad 7 | 40460 | Screenset: Load window set #7 |
| Screenset 8 | Numpad 8 | 40461 | Screenset: Load window set #8 |
| Screenset 9 | Numpad 9 | 40462 | Screenset: Load window set #9 |

## Notes

1. **Verify Action IDs**: Use REAPER's Actions menu (?) to search for actions and verify IDs
2. **SWS Extension**: Some actions require the SWS extension for full functionality
3. **Custom Actions**: Some Logic behaviors may need custom actions/scripts
4. **Numpad**: Logic uses numpad extensively for transport - REAPER can map these

## Sources

- [REAPER Action List](https://www.extremraym.com/cloud/reaper-action-list/)
- [Cockos Forums - Command ID List](https://forum.cockos.com/showthread.php?t=23236)
- [REAPER Accessibility Wiki](https://www.reaperaccessibility.com/wiki/Reaper_shortcut_key_list_by_headings)
