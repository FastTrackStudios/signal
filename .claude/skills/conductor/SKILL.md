---
name: conductor
description: "Orchestrate a graph of ready-for-agent GitHub tickets with parallel Claude worker sessions in Herdr panes — frontier query over blocking edges, one worktree+pane per ticket, review-fix-merge loop per PR. Use when asked to conduct/orchestrate a ticket build-out (e.g. the Files platform #256–#275), resume a paused pipeline, or spawn/monitor/nudge ticket workers. Requires HERDR_ENV=1."
---

# /conductor

Drive a wayfinder ticket graph to closed, as conductor only: workers
implement, reviews find bugs, you route and merge. Never run cargo
yourself; never edit files a worker owns.

Scripts live in `scripts/` beside this file. The worker prompt template
and its judgment notes are in `worker-prompt.md`.

## The loop

Repeat until every ticket in the range is closed:

1. **Frontier** — `scripts/frontier.sh <min> <max>`. Takeable =
   `blocked_by=0` and `assignees=0`. Cap concurrent workers per the
   epic's plan (Files: 3 while the serial spine holds, then 5).
2. **Spawn** — per takeable ticket: `scripts/spawn-worker.sh <ticket>
   <branch-slug> opus`, then fill `worker-prompt.md` (the Build-context
   line is your judgment: merged crates, their review-thread gotchas,
   concurrent-ticket exclusions) and send it: `herdr pane run <pane>
   "<prompt>"` then `herdr pane send-keys <pane> Enter`. Confirm with
   `herdr wait agent-status <pane> --status working`.
3. **Watch** — arm `scripts/monitor-transitions.sh <pane>` under the
   Monitor tool (persistent). Between events, do nothing.
4. **Review** — when a worker's PR is up: `/code-review <pr> high
   --comment` (forked subagent). Triage its findings yourself, then send
   the worker one fix message that names each finding, leads with any
   structural root cause the reviewer identified, and demands regression
   tests, the full fmt+clippy+test gate, a push to the same branch, and
   a reply on every review comment.
5. **Merge** — findings fixed and gate green: merge via
   `gh api -X PUT repos/<owner>/<repo>/pulls/<pr>/merge -f
   merge_method=merge` (plain `gh pr merge` is classifier-blocked).
   Requires standing authorization from Cody — the Files pipeline has it
   (memory: files-platform-merge-policy); without it, report merge-ready
   and stop.
6. **Cleanup** — after the ticket closes: `herdr pane close <pane>`,
   `git worktree remove <worktree>`, back to step 1 — closures open new
   frontier.

Report a status table (ticket → pane → state → PR) to Cody every cycle.

## Stalled workers

A pane leaving `working` is not a stall: `idle` with "N shell still
running" means it is waiting on its own background test run — leave it.
Diagnose from `herdr pane read` before acting; nudge with a specific
`pane run` message; respawn fresh only when truly wedged. A worker
reporting a hanging test wraps up with the test `#[ignore]`d and flagged
in the PR description — the review pass then judges whether the hang is
a real defect to route onward.

## Pane gotchas

- Worker shells are Nushell: `;` between commands, never `&&`.
- Long `pane run` text lands as pasted-text and is NOT submitted —
  always follow with `herdr pane send-keys <pane> Enter`.
- A fresh session may block on a model/credits dialog right after the
  first prompt: `pane read`, then Enter confirms the highlighted choice.
- Monitors: use the Monitor tool with `persistent: true`; plain
  background Bash monitors get killed externally.
- Kill stray processes by exact PID only (live rig — see memory:
  no-pkill-near-live-rig).
