# Worker prompt template

Fill `<N>`, `<WORKTREE>`, and the `Build-context` line, then send as ONE
`herdr pane run` followed by `herdr pane send-keys <pane> Enter` (long
prompts land as pasted text and are not auto-submitted).

```
/mattpocock-skills:implement Files ticket #<N> on FastTrackStudios/FastTrackStudio.
Context: spec = issue #255; engine = ADR 0001 (branch docs/files-platform:
apps/task/docs/adr/0001-files-version-store-jj-cas.md); glossary =
apps/task/CONTEXT.md on that branch — use its vocabulary.
Build-context: <the merged crates this ticket builds on, their PR numbers,
and one line per gotcha from those PRs' review threads. Name any ticket
being worked concurrently and which internals are off-limits because of it.>
First action: gh issue edit <N> --add-assignee @me (that is the claim).
Rules: export CARGO_TARGET_DIR=<WORKTREE>/target-local before any cargo
command; one cargo command at a time; run tests with cargo nextest run
(the tree carries mold+sccache+nextest since PR #281 — direnv provides
them); meet every acceptance criterion on
the ticket; primary test seam per the spec's Testing Decisions; do NOT
run your own code-review skill or workflow — the conductor reviews every
PR after it opens; commit
finished verified work; push your branch and open a PR titled after the
ticket, closing #<N>; comment on the ticket with the PR link when done.
```

The Build-context line is the conductor's judgment call each spawn — it is
what carries lessons from one PR's review into the next worker's head.
