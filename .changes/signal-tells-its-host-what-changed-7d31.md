---
"signal-plugin" = "patch"
---

Signal tells its host when its state changes (CLAP `mark_dirty`), so Studio's autosave reads only the instances that changed, and gives its chain's revisions to the host from any thread (`signal-revisions/1`), so the mixer's live feed never waits for a render (signal)
