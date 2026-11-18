---
"guitar" = "patch"
"sampler" = "patch"
---

Builds no longer compile the checkout's absolute path in (env!(CARGO_MANIFEST_DIR) → utils::paths::checkout_path at run time; guitar's and mic-plugin's build scripts embed files by relative path), so a new worktree builds from the kache store (session)
