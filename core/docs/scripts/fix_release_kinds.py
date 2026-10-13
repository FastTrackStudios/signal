#!/usr/bin/env python3
"""Mark release-combo articulations as @Release in Keyscape styx files.

The external styx generator mis-classified some release articulations as
@OneShot (their names didn't match its heuristic), so the engine's
default-articulation picker grabs a release/attack layer instead of the body
(symptom: "just the attack noise, no sustain").

For each patch this reads the .db manifest to learn each sample's soundsource
role (a `…Release…` DIR = release), maps every sample filename to its runtime
articulation id via the `articulation_of` example (the SAME parser the engine
and styx use — so ids always match, regardless of the patch's naming scheme),
then rewrites `kind @OneShot` -> `@Release` for the articulation ids produced
EXCLUSIVELY by release soundsources (never by a body — guards a body from being
flipped).

Usage: fix_release_kinds.py [--apply] [patch-name ...]
Requires: cargo build -p signal-sampler --release --example articulation_of
"""
import sys, re, os, glob, subprocess

SRC = "/run/media/AudioHaven/SourceLibraries/Keyscape/Keyboards"
RAW = "/run/media/AudioHaven/Sampled/Keys/Keyscape"
# .../<repo>/crates/signal/docs/scripts/this.py -> up 5 to the repo root.
REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), *([os.pardir] * 4)))
ARTIC_BIN = os.environ.get("ARTIC_BIN", os.path.join(REPO, "target/release/examples/articulation_of"))

def db_samples(db):
    """Yield (role, stem) for every audio sample in the manifest.
    role: 'release' if the enclosing top-level soundsource DIR is a release combo."""
    data = open(db, 'rb').read()
    end = data.find(b'</FileSystem>')
    xml = data[:end + 13].decode('latin1')
    toks = re.findall(r'<(/?DIR|FILE)(?:\s+name="([^"]*)")?[^>]*>', xml)
    stack = []
    for kind, name in toks:
        if kind == 'DIR':
            stack.append(name)
        elif kind == '/DIR':
            if stack:
                stack.pop()
        elif name.lower().endswith('.wav') and stack:
            role = 'release' if 'release' in stack[0].lower() else 'body'
            stem = name.rsplit('.', 1)[0]
            yield role, stem

def articulations(stems):
    """Map each stem -> runtime articulation id (batched through the Rust parser)."""
    inp = '\n'.join(stems).encode()
    out = subprocess.run([ARTIC_BIN], input=inp, capture_output=True).stdout.decode()
    return out.splitlines()

def release_only_ids(db):
    rows = list(db_samples(db))
    if not rows:
        return set()
    arts = articulations([s for _, s in rows])
    rel, body = set(), set()
    for (role, _), art in zip(rows, arts):
        if not art:
            continue
        (rel if role == 'release' else body).add(art)
    return rel - body  # release-only ids (never also produced by a body)

def fix_styx(path, ids):
    lines = open(path).read().splitlines()
    out, cur, changed = [], None, []
    for ln in lines:
        m = re.match(r'\s*id ([A-Za-z0-9_]+)\s*$', ln)
        if m:
            cur = m.group(1)
        if re.match(r'\s*kind @OneShot\s*$', ln) and cur in ids:
            out.append(ln.replace('@OneShot', '@Release'))
            changed.append(cur)
            continue
        out.append(ln)
    return '\n'.join(out) + '\n', changed

def main():
    args = sys.argv[1:]
    apply = '--apply' in args
    names = [a for a in args if a != '--apply']
    changed_patches = []
    for db in sorted(glob.glob(f"{SRC}/*.db")):
        patch = os.path.basename(db)[:-3]
        if names and patch not in names:
            continue
        styx = f"{RAW}/{patch}/library.styx"
        if not os.path.exists(styx):
            continue
        ids = release_only_ids(db)
        if not ids:
            continue
        new, changed = fix_styx(styx, ids)
        if changed:
            print(f"{patch}: flip {sorted(set(changed))}")
            changed_patches.append(patch)
            if apply:
                open(styx, 'w').write(new)
    print(f"\n{'APPLIED' if apply else 'DRY-RUN'}: {len(changed_patches)} patches: {changed_patches}")

main()
