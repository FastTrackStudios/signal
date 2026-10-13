#!/usr/bin/env python3
"""The workflow-spec verification gate (`just daw-flows`, CI `checks.yml`).

The standard (issue #48): every `flow.*` rule in docs/spec/session/
workflows.md that is marked implemented (`r[impl flow.x]` somewhere in
the tree) must also be verified (`r[verify flow.x]` on a test). A rule
that has an impl and no verify fails the gate; a rule with neither is
"open" — reported, never failing, so the checklist stays readable.

Data comes from tracey (`tracey query --json rule …`), the same tool
the spec skill uses, so what this prints is what `tracey query status`
counts. Only the rule ids are found here, by scanning the spec files
tracey's config points at — tracey has no "list every rule" query.

Exit status: 0 when every implemented flow rule is verified (or listed
in the grandfather file), 1 otherwise, 2 when tracey itself failed.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

PREFIX = "flow."
CONFIG = ".config/tracey/config.styx"
GRANDFATHERED = ".config/tracey/flow-verify-grandfathered.txt"

# A rule declaration: the paragraph opener `r[flow.some.id]`, with an
# optional `@version` that tracey tracks separately from the base id.
RULE_LINE = re.compile(r"^r\[(" + re.escape(PREFIX) + r"[^\]@\s]+)(?:@[^\]]*)?\]\s*$")


def spec_globs(config_path: str) -> list[str]:
    """The spec `include (...)` globs from tracey's config.

    The config is styx; the one block we need is the spec-level
    `include (` sequence — the first one in the file, before `impls`.
    """
    with open(config_path, encoding="utf-8") as f:
        text = f.read()
    m = re.search(r"\binclude\s*\(([^)]*)\)", text)
    if not m:
        sys.exit(f"{config_path}: no spec include(...) block found")
    return [g for g in m.group(1).split() if not g.startswith("//")]


def declared_rules(root: str, globs: list[str]) -> dict[str, str]:
    """Every `flow.*` rule id declared in the spec files, with its file."""
    rules: dict[str, str] = {}
    for pattern in globs:
        for path in sorted(glob.glob(os.path.join(root, pattern), recursive=True)):
            with open(path, encoding="utf-8") as f:
                for line in f:
                    m = RULE_LINE.match(line.rstrip("\n"))
                    if m:
                        rules[m.group(1)] = os.path.relpath(path, root)
    return rules


class Daemon:
    """A tracey daemon this run owns (`--own-daemon`, the CI mode).

    `tracey query` normally auto-starts a daemon that outlives the
    query, keeps its socket under `$HOME/.local/state/tracey/<hash>/`,
    and answers `{"error": "Cancelled"}` while that daemon is still
    coming up — or forever, if it died. On the runner it dies every
    time: HOME there is long enough that the socket path passes the
    108-byte Unix limit ("path must be shorter than SUN_LEN"), and the
    query has no way to say so. So in CI the script starts the daemon
    itself, under a short XDG_STATE_HOME, with its log in a file that
    is printed if anything goes wrong, and stops it on the way out.
    """

    def __init__(self, root: str) -> None:
        self.root = root
        self.state = tempfile.mkdtemp(prefix="tracey-", dir="/tmp")
        self.env = {**os.environ, "XDG_STATE_HOME": self.state}
        self.log_path = os.path.join(self.state, "daemon.log")
        self.log = open(self.log_path, "w", encoding="utf-8")
        self.proc = subprocess.Popen(
            ["tracey", "daemon", root], stdout=self.log, stderr=subprocess.STDOUT, env=self.env
        )
        # Wait for the socket before the first query: a query that finds
        # no socket auto-starts a second daemon of its own (seen: two
        # strays left behind after a run). Bind takes a few seconds.
        deadline = time.monotonic() + 60
        while not glob.glob(os.path.join(self.state, "tracey", "*", "daemon.sock")):
            if self.proc.poll() is not None:
                fail(self, f"tracey daemon exited with {self.proc.returncode} before serving")
            if time.monotonic() > deadline:
                fail(self, "tracey daemon did not open its socket within 60s")
            time.sleep(0.2)

    def dump_log(self) -> None:
        self.log.flush()
        with open(self.log_path, encoding="utf-8") as f:
            sys.stderr.write(f"--- tracey daemon log ({self.log_path}) ---\n{f.read()}")

    def close(self) -> None:
        # `tracey kill` reaches whatever daemon registered under our
        # state dir (ours, or one a query spawned anyway); terminate
        # covers ours if that message never arrived.
        subprocess.run(["tracey", "kill", self.root], capture_output=True, check=False, env=self.env)
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        self.log.close()
        shutil.rmtree(self.state, ignore_errors=True)


def tracey_json(root: str, *args: str, daemon: Daemon | None = None, budget: float = 120.0):
    """`tracey query --json <root> <args…>`, parsed.

    A `{"error": "Cancelled"}` answer (exit 0) means the daemon is not
    serving yet — retried once a second within the budget, unless the
    daemon is ours and has already exited, which is final. Any other
    error object — an unknown rule id, a config problem — is final.
    """
    env = daemon.env if daemon else None
    cmd = ["tracey", "query", "--json", root, *args]
    deadline = time.monotonic() + budget
    while True:
        proc = subprocess.run(cmd, capture_output=True, text=True, check=False, env=env)
        if proc.returncode != 0:
            sys.stderr.write(proc.stderr)
            fail(daemon, f"tracey query {args[0]}: exit {proc.returncode}")
        data = json.loads(proc.stdout)
        if isinstance(data, dict) and "error" in data:
            if "Cancelled" not in str(data["error"]):
                fail(daemon, f"tracey query {args[0]}: {data['error']}")
            if daemon and daemon.proc.poll() is not None:
                fail(daemon, f"tracey daemon exited with {daemon.proc.returncode}")
            if time.monotonic() > deadline:
                fail(daemon, f"tracey query {args[0]}: still not served after {budget:.0f}s")
            time.sleep(1)
            continue
        return data


def fail(daemon: Daemon | None, message: str) -> None:
    if daemon:
        daemon.dump_log()
    sys.exit(f"::error::{message}")


def tracey_rules(root: str, ids: list[str], daemon: Daemon | None) -> list[dict]:
    """`tracey query --json rule <ids…>` — implRefs/verifyRefs per rule."""
    # Warm the daemon on the cheap query first, so the big one is not
    # what waits out the index.
    status = tracey_json(root, "status", daemon=daemon)
    if not status.get("impls"):
        fail(daemon, f"tracey query status: no impls configured ({CONFIG})")
    data = tracey_json(root, "rule", *ids, daemon=daemon)
    return data if isinstance(data, list) else [data]


def read_grandfathered(path: str) -> set[str]:
    if not os.path.exists(path):
        return set()
    with open(path, encoding="utf-8") as f:
        return {
            line.split("#", 1)[0].strip()
            for line in f
            if line.split("#", 1)[0].strip()
        }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--root", default=os.getcwd(), help="project root (default: cwd)")
    parser.add_argument(
        "--own-daemon",
        action="store_true",
        help="start a private tracey daemon for this run and stop it afterwards (CI)",
    )
    parser.add_argument("--verbose", "-v", action="store_true", help="list every rule by state")
    args = parser.parse_args()
    root = os.path.abspath(args.root)

    declared = declared_rules(root, spec_globs(os.path.join(root, CONFIG)))
    if not declared:
        print(f"::error::no {PREFIX}* rules declared under the spec globs in {CONFIG}")
        return 2
    grandfathered = read_grandfathered(os.path.join(root, GRANDFATHERED))

    daemon = Daemon(root) if args.own_daemon else None
    try:
        details = tracey_rules(root, sorted(declared), daemon)
    finally:
        if daemon:
            daemon.close()

    verified: list[str] = []
    impl_only: list[str] = []
    verify_only: list[str] = []
    open_rules: list[str] = []
    for rule in details:
        rid = rule["id"]["base"]
        has_impl = has_verify = False
        for cov in rule.get("coverage", []):
            has_impl |= bool(cov.get("implRefs"))
            has_verify |= bool(cov.get("verifyRefs"))
        if has_impl and has_verify:
            verified.append(rid)
        elif has_impl:
            impl_only.append(rid)
        elif has_verify:
            verify_only.append(rid)
        else:
            open_rules.append(rid)

    seen = {r["id"]["base"] for r in details}
    missing = sorted(set(declared) - seen)

    failures = [r for r in impl_only if r not in grandfathered]
    excused = [r for r in impl_only if r in grandfathered]
    stale_grandfather = sorted(grandfathered - set(impl_only))

    print(f"{PREFIX}* rules: {len(declared)} declared")
    print(f"  verified (impl + verify): {len(verified)}")
    print(f"  implemented, no verify:   {len(impl_only)}"
          + (f" ({len(excused)} grandfathered)" if excused else ""))
    print(f"  verify only (no impl):    {len(verify_only)}")
    print(f"  open (neither):           {len(open_rules)}")

    if args.verbose:
        for label, group in (
            ("verified", verified),
            ("impl only", impl_only),
            ("verify only", verify_only),
            ("open", open_rules),
        ):
            for rid in sorted(group):
                print(f"    [{label}] {rid}")

    rc = 0
    for rid in sorted(failures):
        print(f"::error file={declared[rid]}::{rid} is r[impl] without an r[verify] — "
              "add `// r[verify " + rid + "]` to the test that proves it "
              f"(or list it in {GRANDFATHERED} and say why)")
        rc = 1
    for rid in sorted(excused):
        print(f"::warning::{rid} is implemented without a verify — grandfathered in {GRANDFATHERED}")
    for rid in stale_grandfather:
        print(f"::warning::{rid} is in {GRANDFATHERED} but no longer needs it — remove the line")
    for rid in missing:
        print(f"::error file={declared[rid]}::{rid} is declared in the spec but tracey does not know it")
        rc = 1
    if rc == 0:
        print("gate: ok — every implemented flow rule has a verify")
    return rc


if __name__ == "__main__":
    sys.exit(main())
