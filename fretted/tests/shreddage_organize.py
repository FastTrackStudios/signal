#!/usr/bin/env python3
"""Shreddage 3.5: move a library's extracted WAVs into the sample layout.

usage: shreddage_organize.py <library root> [--dry-run]

`nkx-extract` writes `<root>/_raw/<container>/Samples/<file>.wav`. This
moves each file (a rename on the same filesystem) to

    <root>/<channel>/<instrument>/<articulation>/<file>.wav

where the file name says all three: `Abyss_Sustain_6_…` (one DI channel),
`Argent_Neck_PM_Str9_Up_…`, `Dingwall_Bridge_SustainLong_Str1_Dn_…`,
`DI_Noise_Neck` (channel = pickup). Every move is appended to
`<root>/_notes/raw_to_organized.tsv` (`<container>.nkx/Samples/<file>`
— the path the decoded zones name — then the organized path), so the
zones tool can map a zone onto its file and the move can be undone.
Files already moved are skipped, so it can be rerun.
"""
import os, re, sys

CHANNELS = ("Neck", "Middle", "Bridge")


def target(name: str) -> tuple[str, str, str]:
    """(channel, instrument, articulation) of one sample file name."""
    stem = name[:-4]
    m = re.match(r"^DI_Noise_(\w+)$", stem)
    if m:
        return m.group(1), "", "DINoise"
    parts = stem.split("_")
    inst = parts[0]
    if len(parts) > 2 and parts[1] in CHANNELS:
        return parts[1], inst, parts[2]
    if inst == "Abyss":
        art = parts[1]
        return "DI", inst, "DINoise" if art == "DI" else art
    raise ValueError(f"unrecognised sample name: {name}")


def main() -> None:
    root = sys.argv[1]
    dry = "--dry-run" in sys.argv
    raw = os.path.join(root, "_raw")
    notes = os.path.join(root, "_notes")
    os.makedirs(notes, exist_ok=True)
    log = os.path.join(notes, "raw_to_organized.tsv")
    done = set()
    if os.path.exists(log):
        with open(log) as f:
            done = {line.split("\t")[0] for line in f if line.strip()}
    moved = 0
    with open(log, "a") as out:
        for container in sorted(os.listdir(raw)):
            sdir = os.path.join(raw, container, "Samples")
            if not os.path.isdir(sdir):
                continue
            for name in sorted(os.listdir(sdir)):
                if not name.endswith(".wav"):
                    continue
                key = f"{container}.nkx/Samples/{name}"
                if key in done:
                    continue
                ch, inst, art = target(name)
                rel = os.path.join(ch, inst, art, name) if inst else os.path.join(ch, art, name)
                if not dry:
                    dst = os.path.join(root, rel)
                    os.makedirs(os.path.dirname(dst), exist_ok=True)
                    if os.path.exists(dst):
                        sys.exit(f"refusing to overwrite {dst}")
                    os.rename(os.path.join(sdir, name), dst)
                    out.write(f"{key}\t{rel}\n")
                moved += 1
    print(f"{root}: {moved} files {'would move' if dry else 'moved'}")


if __name__ == "__main__":
    main()
