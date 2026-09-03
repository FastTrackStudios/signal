#!/usr/bin/env python3
"""Summarise outstanding clippy findings without failing the build.

Reads `cargo clippy --message-format=json` on stdin and writes a Markdown
report to $GITHUB_STEP_SUMMARY (or stdout when running locally). Always exits
0 — this is a visibility gate, not a blocking one.

Two things about how the clippy run must be invoked, both learned the hard way:

  * Pass `-- --cap-lints warn`. The workspace config denies most of clippy, and
    under `deny` a crate ABORTS at its first error, so every crate downstream of
    it is never linted at all. A deny-level count is a floor, not a total:
    measured on this repo the same tree read 3954 / 3892 / 1655 / 684 / 2574
    across consecutive runs depending only on which errors had just been fixed.
    Capping to warn makes the run complete and the number stable.

  * Keep `--all-targets`. A `tests/` or `examples/` file is its own crate, so
    crate-root attributes in `src/lib.rs` do not reach it and its findings are
    otherwise invisible.

Crate-level `#![allow(...)]` still suppresses findings under `--cap-lints warn`
(allow beats the cap), so the DSP crates carrying a rewrite-pending block
correctly report zero here. That is the intent: this report shows work that is
outstanding and unacknowledged, not work already triaged.
"""

from __future__ import annotations

import collections
import json
import os
import sys

# Lints that are a crash or a realtime-safety violation rather than a style
# opinion. Called out separately because they are the reason the strict config
# was adopted, and they should not be lost in a five-figure total.
CRASH_SAFETY = {
    "unwrap_used",
    "expect_used",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
    "exit",
    "panic_in_result_fn",
    "disallowed_methods",
    "indexing_slicing",
    "string_slice",
}


def main() -> int:
    seen: set[tuple[str, str, int, int]] = set()
    by_crate: collections.Counter[str] = collections.Counter()
    by_lint: collections.Counter[str] = collections.Counter()
    crash: collections.Counter[str] = collections.Counter()

    for line in sys.stdin:
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-message":
            continue
        message = msg.get("message", {})
        code = (message.get("code") or {}).get("code") or ""
        if not code.startswith("clippy::"):
            continue
        spans = message.get("spans", [])
        if not spans:
            continue
        span = spans[0]
        lint = code.split("::", 1)[1]
        # One source line reported once, not once per target it compiles into.
        key = (lint, span["file_name"], span["line_start"], span["column_start"])
        if key in seen:
            continue
        seen.add(key)
        by_crate[msg.get("target", {}).get("name", "?")] += 1
        by_lint[lint] += 1
        if lint in CRASH_SAFETY:
            crash[lint] += 1

    total = sum(by_lint.values())
    out = [
        "## Clippy — advisory",
        "",
        f"**{total} outstanding findings** across **{len(by_crate)} crates**.",
        "",
        "This step never fails the build. The strict config "
        "(`pedantic` + `nursery` + panic denies, no allow-list) is adopted and "
        "the tree does not pass it yet; DSP crates carrying a documented "
        "rewrite-pending block report zero here by design.",
        "",
    ]

    if crash:
        out += [
            "### Crash-safety and realtime findings",
            "",
            "These are panics and audio-thread hazards, not style. "
            f"**{sum(crash.values())}** of the total:",
            "",
            "| lint | count |",
            "| --- | ---: |",
        ]
        out += [f"| `{k}` | {v} |" for k, v in crash.most_common()]
        out.append("")

    if by_crate:
        out += ["### By crate", "", "| crate | findings |", "| --- | ---: |"]
        out += [f"| `{k}` | {v} |" for k, v in by_crate.most_common(30)]
        if len(by_crate) > 30:
            out.append(f"| _… {len(by_crate) - 30} more_ | |")
        out.append("")

    if by_lint:
        out += ["### By lint", "", "| lint | count |", "| --- | ---: |"]
        out += [f"| `{k}` | {v} |" for k, v in by_lint.most_common(20)]
        out.append("")

    report = "\n".join(out)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf8") as handle:
            handle.write(report + "\n")
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main())
