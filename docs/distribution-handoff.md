# Distribution & watch-app handoff

Everything about shipping the **FastTrackStudio** and **Task** apps — the build
machine, the pipelines, what's live, and the unfinished **watch-app embedding**
that is the main reason this doc exists. A fresh session should be able to
continue the watch work from section 7.

## 1. What ships today (all working)

| Product | iOS/TestFlight | macOS .dmg | Linux .deb | Server image | watchOS |
|---|---|---|---|---|---|
| **FastTrackStudio** | ✅ live | ✅ notarized | ✅ | — | ✅ embedded companion (build 1784362568) |
| **Task** | ✅ live | ✅ notarized | ✅ | ✅ `docker load` tarball | ⏳ (§7.6) |

GitHub releases (minimal notes — version + downloads only, by request):
- `v0.0.1` — FastTrackStudio (macOS dmg + Linux deb, embed-web)
- `task-v0.0.1` — Task (macOS dmg + Linux deb + `Task-server-0.0.1-docker-image.tar.gz`)

TestFlight builds: FTS `app.fasttrackstudio`, Task `app.fasttrackstudio.task`.
Internal tester `freerunner57@icloud.com` — send the invite email with the
`betaTesterInvitations` API call (adding to a group alone doesn't email; the
API can't ADD a team member to a group — that's UI-only, 409 otherwise).

## 2. The build machine — airlock (Mac mini, ssh user `rat`)

Headless; builds + signs + uploads over SSH. See [[airlock-ios-build-machine]].
- `~/.appstoreconnect/` — ASC API key (`AuthKey_T79FBHC959.p8`) + config.env,
  and the certs' keys/cers: `dist` (Apple Distribution), `devid` (Developer ID
  Application), `dev` (Apple Development — dx `--device` assembly needs this).
- **`fts-build.keychain`** (pw `fts-build`) — holds all three signing
  identities + Apple WWDR G3/G6 + Developer ID G2 intermediates. Headless
  codesigning works because it's a dedicated keychain (see setup-keychain.sh).
- **Two Xcodes**: `/Applications/Xcode.app` (26.6 GM — build/sign/upload) and
  `/Applications/Xcode-beta.app` (27 — its `actool` compiles the icon catalog;
  26.6's is broken on macOS 27). Pass `ACTOOL_DEVELOPER_DIR=…Xcode-beta…`.
- nix at `/nix/var/nix/profiles/default/bin/nix`; `~/fts` is a git checkout.
- GitHub Actions self-hosted runner `airlock` (labels `self-hosted,macOS,airlock`).

**DISK IS TIGHT (recurring).** The APFS container is ~full from macOS + Xcodes +
28 GB of iMessage attachments (`~/Library/Messages` — user's data, sign out to
reclaim) + 24 GB GetGood Drums (`/Users/Shared/GGD`). Each `nix develop` over
SSH **leaks a ~1.7 GB `/private/tmp/nix-shell.*`** dir; they pile up fast.
Before a build campaign: `ssh rat@airlock 'find /private/tmp -maxdepth 1 -name
"nix-shell.*" -exec rm -rf {} +; nix store gc'`. A persistent TMPDIR/gc-root
would fix this permanently — not yet done.

## 3. The pipelines (parametrized, one script per platform)

Both live at `apps/fasttrackstudio/ios/` and are **product-agnostic** via env.

**`deploy-testflight.sh`** (iOS → TestFlight). FTS is the default; for Task:
```
DX_PACKAGE=task-app-mobile DX_APP_DIR=apps/task/mobile DX_FEATURES="" \
DX_BUNDLE_ID=app.fasttrackstudio.task DX_TAILWIND=tailwind.css \
ICONS_DIR=$HOME/fts/apps/task/mobile/ios/Assets.xcassets MARKETING_VER=0.0.1 \
KEYCHAIN=fts-build.keychain KEYCHAIN_PW=fts-build \
ACTOOL_DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
NIX=/nix/var/nix/profiles/default/bin/nix \
bash apps/fasttrackstudio/ios/deploy-testflight.sh
```
`DX_TAILWIND` compiles Tailwind → `assets/tailwind.css` before the build (the
Task mobile sheet was a stub → no CSS; fixed). `SKIP_BUILD=1` reuses the .app.

**`deploy-macos.sh`** (macOS → notarized .dmg). Same DX_* scheme +
`PRODUCT_NAME`, `EMBED_WEB` (1 = bake the LAN web view — FTS; 0 = native — Task),
`DX_TAILWIND`. Developer-ID sign (with `allow-jit` entitlements — phon-jit runs
a JIT), notarize, staple.

**Linux .deb**: `dx bundle --platform linux --package-types deb --release` from
the app dir (AppImage was dropped — dx can't locate linuxdeploy).

**Server image**: `nix build .#task-server-image` → run the streamer →
docker-archive tarball → `docker load`. It's the same image deploy.yml pushes
to starcommand (the server + served web app); the self-host artifact.

## 4. Certs / ids / versions

- Team `28C2G63DA7` (CODY JAMES WRIGHT, individual).
- Bundle ids: `app.fasttrackstudio`, `app.fasttrackstudio.watch`,
  `app.fasttrackstudio.task` (id 5A7XXF2MTU), `app.fasttrackstudio.task.watch`
  (id 8Y5Z8223CC). `app.task` was globally taken.
- ASC app records: FTS `6792116988`, Task `6792234359` ("Task - by
  FastTrackStudio", SKU `task-001`).
- Certs (all in fts-build.keychain, minted via API — mint-*.rb):
  Apple Distribution (TestFlight), Developer ID Application (macOS dmg — was
  Account-Holder-only, user created via CSR), Apple Development (dx `--device`).
- **macOS needs a nightly rustc** for phon-jit's stencils — see
  [[phon-jit-macos-nightly]] (flake provides `PHON_JIT_NIGHTLY_RUSTC`).

## 5. CI

- `ios.yml` — push to main touching `apps/fasttrackstudio/**` etc. → airlock
  runs deploy-testflight.sh → TestFlight. (Only wired for FTS; not Task yet.)
- `release-binaries.yml` — on release publish → macOS dmg (airlock, EMBED_WEB=0)
  + Linux deb (nix-host). **BUG/TODO: it fires on EVERY release and always
  builds FastTrackStudio**, so publishing `task-v*` wrongly builds FTS artifacts
  (currently cancelled by hand). Gate it on `fts-v*` tags, add a Task variant.
- Merges touching `apps/fasttrackstudio/**` re-trigger ios.yml and contend with
  manual airlock builds — cancel the redundant run when doing manual work.
- Tag convention: `fts-v*` and `task-v*` (two products, one repo for now).

## 6. Known gotchas (don't re-discover these)

- Bare `cargo build --target aarch64-apple-ios` fails on `objc2-exception-helper`
  (`xcrun --show-sdk-path` needs `DEVELOPER_DIR`/`SDKROOT` UNSET + xcrun on
  PATH). Always build iOS via the deploy script's env.
- `dx bundle` **requires** `[bundle] identifier` in Dioxus.toml.
- macOS wasm/embed-web: the wasm CC (clang-18) must run with
  `DYLD_LIBRARY_PATH` unset (fixed in toolchain.nix). `wasm-opt` still crashes
  on darwin (same dylib issue) — non-fatal, ships an unoptimized wasm.
- Tailwind: `@source`/`@import` paths must track the monorepo layout
  (`../../../crates/task/…`, `../../../libs/fts-ui/…`); the pre-monorepo
  `FastTrackStudio/fts-ui` paths are dead. Vendor `fts-theme.css` in-tree.

## 7. ⭐ THE WATCH WORK

**Goal:** the watch apps install on the paired watch **via the iPhone app's
TestFlight build** and **auto-update** with every iOS build (not the current
direct `devicectl` push).

**FTS watch: DONE (2026-07-18).** `deploy-testflight.sh` now takes `WATCH_APP`
(+ `WATCH_XCODE_DIR`, `WATCH_SCHEME`, `WATCH_PRODUCT`, `WATCH_BUNDLE_ID`); it
xcodebuilds the watch app unsigned, embeds it at `<iOS.app>/Watch/<product>.app`,
version-locks it to the host, and signs it inside-out with its own App Store
profile before the outer app is sealed. Build 1784362568 uploaded VALID with the
companion embedded — Apple accepted the nested watch bundle. Exact FTS run:
```
KEYCHAIN=fts-build.keychain KEYCHAIN_PW=fts-build \
NIX=/nix/var/nix/profiles/default/bin/nix \
ACTOOL_DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer \
WATCH_XCODE_DIR=/Applications/Xcode-beta.app/Contents/Developer \
WATCH_APP=apps/fasttrackstudio/watchos MARKETING_VER=0.0.1 \
bash apps/fasttrackstudio/ios/deploy-testflight.sh
```
Remaining FTS check: confirm the app offers to install on the paired watch after
installing the TestFlight build (device step). Then do §7.6 (Task watch).

**⚠️ macOS 27 toolchain trap (cost hours — read this).** airlock is on macOS 27.
Do **NOT** pass `XCODE_DIR` to the iOS build. Doing so exports `DEVELOPER_DIR`,
which makes the Rust **host** build scripts (getrandom/num-traits/objc2-…) link
against the **macOS-27 system SDK**, whose `libSystem.tbd` dropped the legacy
`_dyld_image_count`/`_dyld_get_image_*` symbols libstd references →
`ld: symbol(s) not found for architecture arm64`, build dies ~130 crates in, on
BOTH Xcode 26.6 and 27. The proven pipeline leaves `DEVELOPER_DIR` unset so the
host links against the flake's own `apple-sdk-14.4` (still has those symbols);
the iOS *target* crates use Xcode's iOS SDK via `xcrun`. "Xcode 26 doesn't run on
macOS 27" only means its **actool/simulator** GUI pieces are broken (hence
`ACTOOL_DEVELOPER_DIR`=27-beta for icons) — its command-line iOS SDK build is
fine. Keep icons + the watch build on 27 beta (both decoupled via
`ACTOOL_DEVELOPER_DIR` / `WATCH_XCODE_DIR`); leave the main build on the default.
Also: the watchOS **device** platform must be installed for the chosen watch
Xcode (`xcodebuild -downloadPlatform watchOS`) — 27 beta had it, 26.6 needed the
download (~4 GB).

**Done:** `apps/fasttrackstudio/watchos/FTSWatch` (SwiftUI: perform/chords/
session/settings) is converted from standalone (`WKWatchOnly`) to an **embedded
companion** (`WKCompanionAppBundleIdentifier: app.fasttrackstudio` in
project.yml). Tooling exists: `nix run nixpkgs#xcodegen`, `xcodebuild` (26.6).

**The mechanism:** a watch app embedded at `Fasttrackstudio.app/Watch/
<WatchApp>.app` ships inside the iOS `.ipa`; installing the iPhone app from
TestFlight offers the watch app on the paired watch and updates it each build.

**Steps to finish (FTS first, then Task):**
1. **Mint a watchOS App Store provisioning profile** for
   `app.fasttrackstudio.watch` (extend mint-dev-profile.rb — it already does
   IOS_APP_STORE; confirm the profile type covers watchOS, or use the right
   platform). Bundle id must be registered (it is).
2. **Build the watch app**: `nix run nixpkgs#xcodegen` in
   `apps/fasttrackstudio/watchos/` → `xcodebuild archive` for a **generic
   watchOS device**, **Manual** signing with the **Apple Distribution** cert +
   that profile (project.yml is currently `CODE_SIGN_STYLE: Automatic` +
   `-allowProvisioningUpdates` — switch to Manual for the headless App Store
   build). Product name today is `FastTrackStudio` (→ `FastTrackStudio.app`).
3. **Embed**: copy the built watch `.app` into
   `target/dx/fasttrackstudio/release/ios/Fasttrackstudio.app/Watch/`.
4. **Sign inside-out**: re-sign the nested watch `.app` (Distribution + its
   profile + entitlements), then the iOS `.app` (as deploy-testflight.sh already
   does). Then package the `.ipa` and upload.
5. **Wire into `deploy-testflight.sh`** behind a `WATCH_APP` flag (path to the
   watchos project + product name), so `WATCH_APP=…` builds+embeds the companion
   as part of the normal TestFlight run.
6. **Task watch = a NEW app.** There's only the bundle id
   (`app.fasttrackstudio.task.watch`). Create a SwiftUI watch app (model it on
   FTSWatch — Task's list/inbox/today views over its `/watch/v1`-style bridge or
   local CRDT), `WKCompanionAppBundleIdentifier: app.fasttrackstudio.task`, then
   embed it in the Task iOS build via the same `WATCH_APP` path.

**Watch-specific gotchas to expect:** the watch profile's platform/type; the
`.ipa` needs `Payload/App.app/Watch/WatchApp.app` (not `PlugIns`); the watch
app's `MinimumOSVersion`/`WatchOS` deployment (11.0) + `DTPlatformName
watchos` metadata; nested-bundle codesign order (watch first); and altool
validating the watch icon (the watch app needs its own AppIcon).

## 8. Immediate next actions for the new session
1. Free airlock disk (section 2) before building.
2. ✅ FTS watch embedded + uploaded (build 1784362568). Remaining: confirm it
   installs on the paired watch from the TestFlight build (device step).
3. §7.6 — create + embed the **Task** watch app (`app.fasttrackstudio.task.watch`,
   id 8Y5Z8223CC; new SwiftUI app modeled on FTSWatch). Reuse the same
   `WATCH_APP=…` wiring via the Task deploy invocation (§3), pointing `WATCH_APP`
   at the new Task watchos project + `WATCH_BUNDLE_ID=app.fasttrackstudio.task.watch`.
4. Fix the release-binaries `fts-v*` gate + add a Task release job (section 5).

Related memory: [[airlock-ios-build-machine]], [[phon-jit-macos-nightly]],
[[task-distribution]], [[watchos-remote]], [[fts-ci-workflows]].
