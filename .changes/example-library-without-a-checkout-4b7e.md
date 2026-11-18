---
"sampler" = "patch"
---

Fixed a rig with no library configured coming up empty when run from Studio: the shipped example library is compiled in and written to `<config>/signal/example-library` when there is no checkout of this repo to read it from (signal)
