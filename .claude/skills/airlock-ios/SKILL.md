---
name: airlock-ios
description: Build, cross-check, and ship the FastTrackStudio iOS app to TestFlight from the headless airlock Mac mini — source sync, the nix/xcrun env dance, deploy-testflight.sh, disk management, and on-device diagnostics (Logs tab, build number, open_probe). Use when iterating on the iOS app, debugging a TestFlight build, or checking aarch64-apple-ios compilation.
---

# iOS builds on airlock

Airlock is a headless Mac mini (`ssh rat@airlock.local`, macOS 27) that
builds, signs, and uploads TestFlight builds. Two build roots exist:

- `~/fts` — the **manual-iteration snapshot** (rsync'd source, its own
  git history of "airlock build snapshot" commits, warm `target/`).
  This is what you drive over SSH.
- `~/actions-runner/_work/FastTrackStudio/FastTrackStudio` — the CI
  runner checkout (`.github/workflows/ios.yml`, push-to-main = auto
  TestFlight). Leave it alone except to reclaim its `target/` when the
  disk fills.

## Sync source

From the Linux repo checkout (any worktree). If `~/fts` has uncommitted
state from a previous session, snapshot-commit it on airlock first.

```bash
rsync -a --delete --exclude .git --exclude target --exclude web-dist \
  --exclude dist --exclude graphify-out --exclude node_modules \
  --exclude .direnv --exclude "*.signalpack" ./ rat@airlock.local:fts/
```

## The env dance (REQUIRED for anything targeting iOS)

nixpkgs ships a fake xcbuild `xcrun` and the devshell's `SDKROOT`
points at the macOS SDK, so iOS cross-compiles need the real `xcrun`
first on PATH and the nix SDK vars unset. The flake's
`CARGO_TARGET_AARCH64_APPLE_IOS_LINKER` / `CC_aarch64_apple_ios`
wrappers handle the rest. `build-ios.sh` and `deploy-testflight.sh` do
this themselves; for ad-hoc cargo commands:

```bash
ssh rat@airlock.local
export PATH="/nix/var/nix/profiles/default/bin:$PATH"
cd ~/fts && nix develop -c bash -c '
  mkdir -p $HOME/bin-ios; ln -sf /usr/bin/xcrun $HOME/bin-ios/xcrun
  export PATH="$HOME/bin-ios:$PATH"; unset DEVELOPER_DIR SDKROOT
  cargo check --target aarch64-apple-ios -p fasttrackstudio \
    --no-default-features --features signal-guitar,signal-keys-rig'
```

Never set `DEVELOPER_DIR` for the main build (macOS-27 host-SDK trap:
host build scripts must link the flake apple-sdk, see the
fts-ios-macos27-host-sdk memory). `ACTOOL_DEVELOPER_DIR` may point at
the Xcode beta for icon compilation only.

## Ship a TestFlight build

```bash
ssh rat@airlock.local 'export PATH="/nix/var/nix/profiles/default/bin:$PATH"; \
  export NIX=/nix/var/nix/profiles/default/bin/nix; \
  export KEYCHAIN=fts-build.keychain KEYCHAIN_PW=fts-build \
         MARKETING_VER=0.0.1 \
         ACTOOL_DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer; \
  cd ~/fts && bash apps/fasttrackstudio/ios/deploy-testflight.sh 2>&1 | tail -4'
```

- Prints `build <unix-time>` — that number IS the TestFlight build id;
  tell the tester which one to install. The app shows its own build
  number on the Keys bar (`b17849…`) so screenshots are unambiguous.
- Apple processes uploads in ~5–15 min. Builds are cheap; when in doubt
  ship another rather than wonder which binary the tester has.
- Ignore a mid-log `dx build … task-app-mobile` error — dx also tries
  the Task mobile app in the workspace; only the fasttrackstudio .app
  matters and the script gates on it.

## Disk (chronically ~full, 228 GB)

Safe reclaims, in order: `~/Library/Caches` (a few GB), the CI runner's
`target/` (~20 GB, cold-rebuilds itself), `~/fts/target/debug` +
`~/fts/target/aarch64-apple-ios/debug` (keep `release` for incremental
deploys). NEVER `nix-collect-garbage` with a date filter — it deletes
the devshell closure and the next build spends 30+ min and ~9 GB
rebuilding toolchains. `~/Library/Messages` (18 GB) is personal data —
do not touch.

## Debugging a build that misbehaves on the phone

1. Confirm the build number on the Keys bar matches what you shipped.
2. Keys → **Logs** tab = live tracing + panics (`src/log_ring.rs`),
   Copy-all → paste. Audio-open failures also surface on the Play tab
   (`KeysStatus.last_error`, includes caught panics).
3. Reproduce engine-side issues headless on Linux first:
   `cargo run -p signal-keys --example open_probe` with
   `FTS_KEYSCAPE_PACKS=<dir-with-a-pack>` — it walks the exact phone
   bring-up (scan → auto-start → open audio → load default preset).
4. Known iOS traps already fixed once — check before re-introducing:
   cfg!() in build scripts is the HOST not the target (reaper-low);
   per-OS cfg blocks need mobile arms (swell-ui); `~/.config` is not
   writable in the container (use `XDG_CONFIG_HOME`, set in main.rs);
   `BufferSize::Fixed` rides a macOS-only CoreAudio property; threads
   that reach the daw-standalone engine must enter a tokio runtime
   (`keys_runtime().enter()`); dropping an `iroh::Endpoint` closes all
   its connections (hold it in a static).
5. The Local Network prompt only fires via the Bonjour poke
   (`request_local_network()`, `NSBonjourServices` in Info.plist) —
   without user approval LAN paths stay blocked and iroh rides relays.

## Related

Pack distribution architecture: `crates/signal/docs/pack-distribution.md`.
Device deploy without TestFlight: `apps/fasttrackstudio/ios/deploy-iphone.sh`.
Simulator/dev build: `apps/fasttrackstudio/ios/build-ios.sh`.
