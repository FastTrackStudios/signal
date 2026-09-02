# Running measurements on voyager

Voyager is the Mac (`ssh voyager`, macOS arm64) that holds the plugins this
Linux box does not — the whole UADx library, SSL Native, and authorised
FabFilter. Plugins are **native VST3/CLAP**, not yabridge, so there is no
bridge to blame when something misbehaves.

```
/Library/Audio/Plug-Ins/VST3/       UADx, SSL
/Library/Audio/Plug-Ins/CLAP/       FabFilter
~/Documents/FabFilter/Presets/      preset libraries
~/Development/signal-measure/       the source checkout
```

## The loop

```sh
rsync -a --exclude=target --exclude=.git ./features/analyzer/ \
    voyager:Development/signal-measure/features/analyzer/
ssh voyager 'cd ~/Development/signal-measure &&
    touch features/analyzer/signal-analyzer/src/*.rs \
          features/analyzer/signal-analyzer/examples/*.rs &&
    cargo build --release -p signal-analyzer --example saturation_capture'
```

**The `touch` is not optional.** `rsync -a` preserves modification times, so
cargo compares a freshly synced source against an older-looking timestamp,
decides it is up to date, and silently runs the previous binary. This has
already caused one round of measurements to be taken with stale code, which
is invisible in the output.

## Practicalities

- **Disk.** The data volume runs close to full. `df -h /System/Volumes/Data`
  before a big capture; stale `target/` dirs under `~/Development` are the
  reclaimable bulk.
- **Cores.** 10. `--threads 10` is the right setting for captures; more
  instances do not help, since rendering is CPU-bound. The host will hold a
  thousand plugin instances without complaint, but that is a robustness
  property, not a throughput one.
- **UADx logging.** These plugins print a great deal to stderr. Redirect it
  or the useful output is lost in `Async@warn` lines.
- **Parameter layout.** UADx exposes ~2091 parameters; the real controls
  start at index 48 and there are eight of them. SSL uses non-sequential
  ids. Resolve **by name** in both cases.
- **Latency.** UADx units report 86–87 frames, SSL reports 0.
  `transfer_curve` cross-correlates rather than trusting the report.
- **Quoting.** Plugin and preset paths contain spaces. An unquoted
  `--out /tmp/x-Pro-C 2` silently splits into two arguments and writes
  somewhere unexpected; it cost a confusing round of "the plugin crashed"
  before the real cause turned out to be a shell split.
- **Check nothing is already running** before starting a batch. Two capture
  runs writing the same output directory interleave their results, and the
  mixture is not detectable afterwards.
