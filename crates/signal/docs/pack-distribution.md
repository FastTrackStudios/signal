# Pack distribution — proxy packs, p2p, and the hosted mirror

How a `.signalpack` gets from the studio drives onto a phone (or any
remote device) and becomes a playable patch. Built 2026-07-24 on
`worktree/mobile-keys-rig`; the mobile keys rig is the first consumer.

## The pieces

```
Full packs (FLAC)          Proxy packs (Ogg q0.6, ~5.5-6x smaller)
/…/Signal/Libraries/…  →   /…/Signal/Libraries/Proxy/…   (same layout)
        │ transcode_pack example (no source samples needed)
        ▼
signal-pack-library ──── mounted on the engine's /vox router
  (scan + sha256 + chunk streaming)   │
        │            ┌────────────────┴───────────────┐
        │            ▼                                ▼
        │     ws://host:4040/vox              iroh p2p (dumbpipe-style:
        │     (LAN / hosted)                  bare endpoint id, n0 relays
        │                                     + DNS discovery + holepunch)
        ▼
HTTPS mirror (backup): task-server public media route
  https://fasttrackstudio.app/org/fasttrackstudios/media/packs/…
```

### Proxy packs

A proxy pack is a byte-layout twin of a Full pack with Ogg Vorbis
entries instead of FLAC (`PackCodec::OggVorbis`, header kind 6). The
index keeps the source PCM frame counts, so loop points and zone
metadata stay sample-exact and `check_pack_resolve` reports identically
to the Full twin. Build one from a Full pack with:

```bash
cargo run -p signal-sampler --release --example transcode_pack -- \
    "<in.signalpack>" "<out.signalpack>" 0.6   # ~23 min for the 14 GB C7
```

Run transcodes `nice -n 19` — the encoder saturates every core.
Keyscape keys proxies: C7 Grand 2.6 GB, LA Custom Rhodes 441 MB,
Wurlitzer 200A 96 MB.

### Wire contract — `signal-packs-proto`

`packs::PackLibrary`: `packs()` lists `PackInfo` rows (name, category,
variant `proxy|full`, size, sha256 — empty while the host still
hashes); `read(name, variant, start, tx)` streams 256 KiB `PackChunk`s
from `start` to EOF (resume = re-call with the `.part` length). Wire
types carry manual `vox_types::Reborrow` impls (streamed payloads cross
lanes as `SelfRef<T>` — media-proto is the same pattern).

### Host — `signal-pack-library`

`PackLibraryBackend::new()` scans `FTS_PACK_LIBRARY` (default
`/run/media/AudioHaven/Signal/Libraries`); a `Proxy/` path component
maps to variant `proxy` with the prefix stripped, so twins share a
category. Only proxy variants are hashed eagerly (sidecars
`<pack>.signalpack.sha256`, `size:hex`) — full trees are multi-TB.
Mounted in `engine_main.rs` via `merge_router`, so ws and iroh serve it
with zero extra wiring. A scratch pack host (never touch the live rig):

```bash
XDG_CONFIG_HOME=~/.local/share/fts-pack-host SIGNAL_ENGINE_ADDR=0.0.0.0:4141 \
FTS_PACK_LIBRARY=/run/media/AudioHaven/Signal/Libraries \
  fasttrackstudio --engine   # prints its iroh endpoint id; key persists in that config dir
```

`pack_probe` (fasttrackstudio example) lists/downloads/verifies against
a ws URL or a bare iroh endpoint id; `iroh_echo` bisects transport
problems (raw frames vs vox). Both require `--no-default-features
--features signal-guitar`. Iroh clients MUST keep their `Endpoint`
alive for the whole session — dropping it closes every connection —
and loopback self-dial is pathological, so verify iroh cross-machine.

### HTTPS mirror (backup path)

The prod task-server serves org resources publicly with Range support:
packs + `packs.json` live at
`/data/orgs/fasttrackstudios/resources/packs/` (PVC on starcommand
k3s), reachable via `https://fasttrackstudio.app/org/…` (the edge is
CADDY — `kubectl -n ingress get cm caddy-config`; the
fasttrackstudio.app block path-splits `/vox /org /server /blobs
/.well-known` to task-server; the traefik ingressClass is vestigial)
and `https://task.starcommand.live/org/…`. `packs.json` rows must
match `PackInfo` fields exactly (facet-json). Update the mirror by
`kubectl exec -i … -- sh -c 'cat > …'` streaming from the studio.

### App client — `pack_client.rs` + `keys_view.rs`

The Library page tries the vox host (saved ws URL or iroh id;
`remote.rs` bakes the studio endpoint id as the iOS default —
TEMPORARY, remove with host-pairing UX) and falls back to the mirror
(ranged 16 MiB GETs). Both paths share `.part` resume, an in-flight
guard (one transfer per pack — concurrent appends corrupt the part
file), pause/resume, sha256 verify, and land in the keys packs dir
(`FTS_KEYSCAPE_PACKS`; on iOS `Documents/FastTrackStudio/Packs/Keys`,
Files-app visible). On completion the rig `rescan()`s and auto-starts;
the default patch is the LA Custom Rhodes, else the first pack.

### Phone specifics

- `FTS_PRELOAD_PROFILE=fast-audition` (set in main.rs on iOS): 64
  samples decode eagerly, the rest at first note-on — a full piano
  decoded is more RAM than the device has.
- Keys backend worker threads enter `keys_runtime()` (the
  daw-standalone engine spawns tokio tasks during open).
- iOS: default IO buffer (never `Fixed`), config under
  `XDG_CONFIG_HOME`, Local Network permission raised via a Bonjour
  poke. See the `airlock-ios` skill for the build/debug loop and the
  full iOS trap list.

## Open follow-ups

- Fold the Caddyfile `/org` split + mirror uploads into the GitOps
  source (hand-applied on the cluster 2026-07-24).
- True background downloads on iOS (background URLSession bridge; the
  in-app keep-awake is the interim).
- Streaming playback from partially-downloaded packs; per-zone lazy
  loading beyond decode-on-demand.
- Host pairing UX replacing the baked-in default endpoint id.
- Auth on the pack routes if the library ever hosts non-owned content.
