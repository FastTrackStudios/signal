#!/usr/bin/env python3
"""Mark release-combo articulations as @Release in Keyscape styx files.

The external styx generator mis-classified some release articulations as
@OneShot (their names, e.g. `fr`/`mr`, didn't match its heuristic), so the
engine's default-articulation picker grabs a release/attack layer instead of
the body. This reads each patch's .db manifest, finds RELEASE soundsources,
computes their styx article-ids, and rewrites the styx `kind @OneShot` ->
`kind @Release` for exactly those articulations. Body articulations are left
untouched.

Usage: fix_release_kinds.py [--apply] [patch-name ...]
"""
import sys, re, os, glob

SRC = "/run/media/AudioHaven/SourceLibraries/Keyscape/Keyboards"
RAW = "/run/media/AudioHaven/Sampled/Keys/Keyscape"

def artid(name):
    base = name.rsplit('.', 1)[0]
    parts = base.split(' ')
    toks = parts[1:] if len(parts) > 1 else parts
    out = ''
    for t in toks:
        head = t.split('_')[0]
        if head[:1].isdigit():
            break
        alpha = ''.join(c for c in head if c.isalnum())
        if alpha[:1].isdigit():
            break
        out += alpha.lower()
        if '_' in t:
            break
    return out

def release_articles(db):
    """Article-ids produced EXCLUSIVELY by release soundsources (never by a body
    soundsource) — so we never flip a body articulation to @Release."""
    data = open(db, 'rb').read()
    end = data.find(b'</FileSystem>')
    xml = data[:end + 13].decode('latin1')
    toks = re.findall(r'<(/?DIR|FILE)(?:\s+name="([^"]*)")?[^>]*>', xml)
    stack = []
    rel, body = set(), set()
    for kind, name in toks:
        if kind == 'DIR':
            stack.append(name)
        elif kind == '/DIR':
            if stack:
                stack.pop()
        elif name.lower().endswith('.wav') and stack:
            ss = stack[0].lower()
            a = artid(name)
            if not a:
                continue
            if 'release' in ss:
                rel.add(a)
            else:
                # body / mechanical / pedal — anything that is NOT a release combo
                body.add(a)
    return rel - body  # release-only ids

def fix_styx(path, rel_ids):
    """Set `kind @OneShot` -> `@Release` for articulations whose id is in rel_ids."""
    lines = open(path).read().splitlines()
    out = []
    cur = None
    changed = []
    for ln in lines:
        m = re.match(r'\s*id ([A-Za-z0-9_]+)\s*$', ln)
        if m:
            cur = m.group(1)
        if re.match(r'\s*kind @OneShot\s*$', ln) and cur in rel_ids:
            out.append(ln.replace('@OneShot', '@Release'))
            changed.append(cur)
            continue
        out.append(ln)
    return '\n'.join(out) + '\n', changed

def main():
    args = sys.argv[1:]
    apply = '--apply' in args
    names = [a for a in args if a != '--apply']
    dbs = sorted(glob.glob(f"{SRC}/*.db"))
    total_changed = []
    for db in dbs:
        patch = os.path.basename(db)[:-3]
        if names and patch not in names:
            continue
        styx = f"{RAW}/{patch}/library.styx"
        if not os.path.exists(styx):
            continue
        rel = release_articles(db)
        if not rel:
            continue
        new, changed = fix_styx(styx, rel)
        # only report ids that were actually @OneShot and got flipped
        if changed:
            print(f"{patch}: release-articles={sorted(rel)}  -> flip {sorted(set(changed))}")
            total_changed.append(patch)
            if apply:
                open(styx, 'w').write(new)
    print(f"\n{'APPLIED' if apply else 'DRY-RUN'}: {len(total_changed)} patches would change: {total_changed}")

main()
