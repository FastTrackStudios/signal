#!/usr/bin/env python3
"""Wrap a single Omnisphere patch (.prt_omn) into a Multi XML by splicing its
<SynthEngine> subtree into Part 1 of a template Multi (a state dump's XML or
a .mlt_omn). Optionally rewrite attributes first — the calibration hook.

Usage:
  omni_patch_to_multi.py <patch.prt_omn> <template_multi.xml> <out.xml> [ATTR=HEXVAL ...]

Attribute rewrites apply to the PATCH xml before splicing and replace every
occurrence of `ATTR="..."` with `ATTR="HEXVAL"` (values are the raw attribute
strings — IEEE-754 hex for floats, e.g. `rels=3f000000`).
"""

import re
import sys


def extract_engine(patch_xml: str) -> str:
    start = patch_xml.index("<SynthEngine")
    end = patch_xml.rindex("</SynthEngine>") + len("</SynthEngine>")
    return patch_xml[start:end]


def splice_part0(multi_xml: str, engine_xml: str) -> str:
    sub = multi_xml.index("<SynthSubEngine")
    start = multi_xml.index("<SynthEngine", sub)
    end = multi_xml.index("</SynthEngine>", start) + len("</SynthEngine>")
    return multi_xml[:start] + engine_xml + multi_xml[end:]


def main() -> None:
    patch_path, template_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    patch = open(patch_path, errors="replace").read()
    for rewrite in sys.argv[4:]:
        attr, val = rewrite.split("=", 1)
        patch, n = re.subn(rf'{attr}="[^"]*"', f'{attr}="{val}"', patch)
        print(f"  {attr} -> {val} ({n} sites)")
    template = open(template_path, "rb").read().rstrip(b"\x00 ").decode(errors="replace")
    out = splice_part0(template, extract_engine(patch))
    open(out_path, "w").write(out)
    print(f"{out_path}: {len(out)} chars")


if __name__ == "__main__":
    main()
