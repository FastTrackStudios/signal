# Workstation DOM stress benchmark

See the [developer and agent guide](GUIDE.md) for baseline comparisons, phase isolation, crash diagnosis and interpretation.

`just ee-stress` copies Set in Stone and its audio to a retained temporary practice directory, mounts the real `WorkstationApp` in `dioxus-test`, waits for track/item loading and waveform previews, and dispatches input through the Blitz DOM event handler. No window, desktop focus, mouse automation, keyboard automation, or audio device is required.

```sh
just ee-stress set-in-stone 120 dev
just ee-stress unbreakable 240 release true
```

Arguments: song, frames per phase, Cargo profile, enforce budget. The last command fails if any measured DOM frame exceeds 8.333 ms. Source projects and audio are never edited; practice copies and reports remain in temporary directories printed by the command. Use `EXPRESSION_EDITOR_PRACTICE_ALBUM` for a different album location and `TMPDIR` for temporary storage.

Every navigation gesture is checked for a changed pane before timing; a failed check aborts and saves a diagnostic screenshot. The mounted TCP/arrange, MCP and drum editor receive vertical/horizontal wheel input, arrange zoom, drum pan and held-Z zoom in alternating directions, then together. Input uses `DocumentTester` and its public `send_ui_event`, exercising actual hit testing, bubbling, focus and default actions. It never modifies editor state to simulate navigation.

Reports contain every event/update/layout sample, per-stage percentiles, worst cases and counts over budget, plus project, viewport, CPU, load, build profile and revision context. `workstation.png` is rendered outside timing. Frames are driven as fast as possible, not paced by sleep. Compare the same song, viewport, profile, input configuration and frame count on an otherwise idle machine.

**These timings exclude painting and presentation.** Passing the DOM budget is necessary but cannot establish a consistent 120 FPS display experience. The renderer still needs time within that same budget. Debug and release results must not be compared as performance improvements; headless CPU screenshot rendering also must not be presented as native GPU FPS.

For an existing scratch project, build `--example stress`, run it with `FTS_STRESS_FRAMES=120 FTS_STRESS_PROFILE=dev` and the regular `--drums --size 1600x900 --out /tmp/report` arguments, then run `python3 scripts/ui-stress/run.py /tmp/report`. Do not include project loading or PNG encoding in frame timings.

Check report accounting with `python3 -m unittest discover -s scripts/ui-stress -p 'test_*.py'`.
