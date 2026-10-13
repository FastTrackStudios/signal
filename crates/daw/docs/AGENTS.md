# Agent Instructions

This project uses **GitHub Issues** for issue tracking.

## Quick Reference

```bash
gh issue list --state open
gh issue view <number>
gh issue create --title "Issue title" --body "Context and acceptance criteria"
gh issue close <number> --comment "Completed in <commit-or-pr>"
```

## btca — Source Code Search

Use **btca** to query the actual source code of key dependencies before implementing features or debugging. Prefer this over web searches or docs that may be outdated.

```bash
btca ask -r <resource> -q "your question"
btca ask -r facet -r roam -q "How does roam generate TypeScript clients from Rust traits?"
btca resources   # list all available resources
```

### Relevant Resources for This Repo

| Resource | Repo | Description |
|----------|------|-------------|
| `facet` | facet-rs/facet | Rust reflection — shapes, derive macros, serialization, pretty-printing |
| `roam` | bearcove/roam | Rust-native RPC framework where Rust traits are the schema, with TS/Swift codegen |
| `tracey` | bearcove/tracey | Traceability tool linking requirements/specs to code implementations via annotations |
| `figue` | bearcove/figue | Config parsing from CLI args, env vars, and config files using facet reflection |
| `styx` | bearcove/styx | Cleaner serialization format — alternative to JSON/YAML with schema support |
| `reaper-rs` | helgoboss/reaper-rs | Low/medium/high-level Rust bindings for the REAPER DAW API (reaper-low, reaper-medium, reaper-high) |
| `rea-rs` | Levitanus/rea-rs | Higher-level idiomatic Rust wrapper around the REAPER C++ API |
| `helgobox` | helgoboss/helgobox | Full REAPER extension (ReaLearn/Playtime) — reference for building complex REAPER plugins in Rust |
| `sws` | reaper-oss/sws | SWS/S&M Extension — large open-source REAPER extension with actions, snapshots, and API extensions |

When working on REAPER extension behavior, especially action registration,
toggle states, toolbar refresh, menu integration, or command invocation, query
`helgobox` and `sws` with `btca` before changing code. Prefer those repositories
as source-code references for how production extensions use the REAPER API.

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

## Issue Tracking with GitHub Issues

**IMPORTANT**: Use GitHub Issues for follow-up work, bugs, and feature requests.

### Workflow for AI Agents

1. **Check open work**: `gh issue list --state open`
2. **Inspect context**: `gh issue view <number>`
3. **Create follow-ups** with a clear title, body, and acceptance criteria:
   ```bash
   gh issue create --title "Issue title" --body "Context, proposed fix, and acceptance criteria"
   ```
4. **Reference issues in commits/PRs** when relevant.
5. **Close completed issues** only after the fix is committed and pushed:
   ```bash
   gh issue close <number> --comment "Completed in <commit-or-pr>"
   ```

### Important Rules

- Use GitHub Issues for task tracking.
- Do not add legacy local issue-tracker instructions.
- Do not create markdown TODO lists as a substitute for tracked issues.
