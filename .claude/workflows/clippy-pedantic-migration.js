export const meta = {
  name: 'clippy-pedantic-migration',
  description: 'Iteratively fix cargo clippy pedantic/nursery/panic-lint denials using fast fixer agents',
  whenToUse: 'A Rust repo already has (or just got) a strict clippy config (pedantic+nursery deny, unwrap/expect/indexing/arithmetic denies) and cargo clippy now fails with many errors that need fixing file-by-file.',
  phases: [
    { title: 'Discover', detail: 'run cargo clippy, group errors by file' },
    { title: 'Fix', detail: 'bin-packed batches, edits only, no Bash' },
    { title: 'Verify', detail: 're-run clippy, loop until clean or stalled' },
  ],
}

// args: {
//   cwd: string (required) — absolute path to the repo root to run cargo in
//   package: string | null — cargo -p target; null/omitted = whole workspace
//   extraClippyArgs: string — appended to the cargo clippy invocation (default '')
//   fixerModel: 'haiku' | 'sonnet' | 'opus' | 'fable' (default 'haiku')
//   maxRounds: number (default 4)
//   maxErrorsPerAgent: number (default 60) — hard cap on errors handed to one
//     agent. Overflow is deferred to the next round, NOT dropped.
//   maxCharsPerAgent: number (default 24000) — second cap, on prompt size.
//   issuesPerAgent: number (default 10) — bin-packing target. Files with fewer
//     errors than this are grouped so one agent fixes ~this many issues across
//     several small files, instead of one agent per file.
// }
//
// WHY BIN-PACKING IS BY FILE, NEVER WITHIN A FILE:
// Most offending files have 1-3 errors, so one-agent-per-file spent a whole
// agent round-trip on a single doc backtick. Batching amortizes that. But a
// file is the unit of exclusivity: Edit is a read-modify-write on file text, so
// two agents editing one file concurrently clobber each other. Every file
// therefore lands in exactly ONE batch. Files at or above the target keep a
// dedicated agent, with overflow deferred to a later round — which serializes
// them across rounds rather than racing them within one.
//
// WHY THE FIXERS HAVE NO BASH (see agentType below):
// One workspace = one target/ = one build lock. A fixer that runs cargo queues
// behind the Discover stage's own clippy run, prints "Blocking waiting for file
// lock", and idles ~30 minutes looking exactly like a hang. That is what went
// wrong on the first run of this workflow: the prompt said "do not run cargo",
// and cheap models ran cargo anyway. Prompts are not a sandbox. The fixers now
// use the `clippy-fixer` agent type, whose frontmatter grants Read/Edit/Write/
// Grep and no Bash, so the failure is structurally impossible rather than
// merely forbidden. agent() has no timeout option and the script sandbox has no
// timers, so removing the cause is the only real lever available here.

const cwd = args.cwd
if (!cwd) throw new Error('args.cwd is required — absolute path to the repo root')
const pkgFlag = args.package
  ? `-p ${args.package} --no-deps --all-targets --keep-going`
  : '--workspace --no-deps --all-targets --keep-going'
const extra = args.extraClippyArgs || ''
const fixerModel = args.fixerModel || 'haiku'
const maxRounds = args.maxRounds || 4
const maxErrorsPerAgent = args.maxErrorsPerAgent || 60
const maxCharsPerAgent = args.maxCharsPerAgent || 24000
const issuesPerAgent = args.issuesPerAgent || 10

const DISCOVER_SCHEMA = {
  type: 'object',
  properties: {
    totalErrors: { type: 'number' },
    files: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          path: { type: 'string' },
          count: { type: 'number' },
          errorBlocks: { type: 'array', items: { type: 'string' } },
        },
        required: ['path', 'count', 'errorBlocks'],
      },
    },
  },
  required: ['totalErrors', 'files'],
}

const FIX_SCHEMA = {
  type: 'object',
  properties: {
    path: { type: 'string' },
    changed: { type: 'boolean' },
    summary: { type: 'string' },
  },
  required: ['path', 'changed', 'summary'],
}

function discoverPrompt() {
  return `Run this exact shell pipeline from ${cwd} and report the result.

1. cd ${cwd}
2. Run: timeout 1800 cargo clippy ${pkgFlag} ${extra} > /tmp/clippy_migration_out.log 2>&1 ; true
   (If another cargo process holds the build lock this will sit silently. That
   is expected and it will proceed; do not start a second cargo command.)
3. Run this Python script (via \`python3 - <<'PY' ... PY\`) to group the errors by file into JSON, and print ONLY the JSON to stdout:

import re, json
with open("/tmp/clippy_migration_out.log") as f:
    log = f.read()
blocks = re.split(r'\\n(?=error(?:\\[|:))', log)
by_file = {}
for b in blocks:
    if not b.startswith("error"):
        continue
    m = re.search(r'--> (\\S+):(\\d+):(\\d+)', b)
    if not m:
        continue
    by_file.setdefault(m.group(1), []).append(b.strip())
files = [{"path": p, "count": len(v), "errorBlocks": v} for p, v in by_file.items()]
files.sort(key=lambda f: -f["count"])
total = sum(f["count"] for f in files)
print(json.dumps({"totalErrors": total, "files": files}))

4. Report back the exact JSON object the Python script printed, matching the required schema. Do not summarize, truncate, or reformat the "errorBlocks" entries — each must be one full raw clippy error block, verbatim, so a fixer agent can act on it without re-running clippy.`
}

function fixPrompt(units) {
  const multi = units.length > 1
  const total = units.reduce((n, u) => n + u.blocks.length, 0)
  const header = multi
    ? `You are fixing ${total} cargo clippy lint violations spread across ${units.length} files. Only use Read and Edit (or Write), and only on the files listed below. You have no Bash tool: verification is a later stage's job.

Repo root: ${cwd}

Work through the files one at a time, in the order given. They are independent — no other agent is touching any of them.`
    : `You are fixing cargo clippy lint violations in exactly one file. Only use Read and Edit (or Write) on this one file. You have no Bash tool: verification is a later stage's job.

Repo root: ${cwd}`

  const body = units
    .map((u) => {
      const deferred = u.file.count - u.blocks.length
      const note = deferred > 0
        ? `\n(This file has ${u.file.count} errors in total; you are given the first ${u.blocks.length}. The remaining ${deferred} go to a later round. Fix only the ones listed here.)`
        : ''
      return `${multi ? '========================================\n' : ''}File: ${u.file.path} (relative to the repo root above)${note}

Errors reported for this file (line numbers refer to the file's current state on disk):

${u.blocks.join('\n\n')}`
    })
    .join('\n\n')

  return `${header}

${body}

Fix every one of them by editing the file directly with a REAL fix. Never use \`#[allow(clippy::...)]\` (item, function, module, or file level) to satisfy a lint — not even when the flagged code looks safe/bounded. Always rewrite instead:
- \`checked_add\`/\`checked_sub\`/\`saturating_add\`/\`saturating_sub\`/\`saturating_neg\`/\`saturating_mul\` instead of raw +, -, *, unary - on ints.
- \`.get(i)\`/\`.get_mut(i)\`/\`.get(a..b)\` (with \`.unwrap_or(...)\`, \`?\`, or an \`else\` branch) instead of \`v[i]\`/\`&v[a..b]\` indexing/slicing — including on \`&str\`, where \`.get()\` returns \`Option<&str>\`.
- \`i32::try_from(x).unwrap_or(FALLBACK)\` (a real, non-panicking fallback value, never \`.unwrap()\`) instead of \`x as i32\`-style casts. For an enum-to-integer cast on a small fieldless enum, prefer an explicit \`match\` returning the integer per variant over \`as\`.
- \`Option::map_or\`/\`map_or_else\` instead of \`if let Some(x) = opt {..} else {..}\`.
- Add missing \`# Errors\`/\`# Panics\` doc sections truthfully (state what actually causes the error/panic).
- \`const fn\` where clippy suggests it and it's legal (all called functions must also be const).
- \`x.clone_from(&y)\` instead of \`x = y.clone()\`.
- For \`too_many_lines\`: actually split the function into smaller named helpers (or, for a big literal data table, reformat entries onto fewer lines) — don't just suppress.
- For \`panic_in_result_fn\` inside a \`#[test]\`/\`#[tokio::test]\` function: change the function to NOT return \`Result\` (return \`()\`) and \`.unwrap()\` the fallible setup calls instead — \`.unwrap()\`/\`.expect()\` ARE allowed inside a function directly marked \`#[test]\`/\`#[tokio::test]\`/\`#[cfg(test)]\`, just not one that also returns \`Result\` and uses \`assert!\`/\`assert_eq!\`. **STOP — do NOT do this if the function is annotated with any OTHER attribute** (a custom macro like \`#[reaper_test]\`, or anything not literally \`#[test]\`/\`#[tokio::test]\`/\`#[rstest]\`) **or if its signature was already \`-> Result<(), SomeErrorType>\` before you started.** A custom test-harness macro typically wraps the function and requires that exact \`Result\`-returning signature — stripping it breaks the macro with a confusing \`Pin<Box<dyn Future<Output = Result<...>>>>\` type-mismatch, and clippy's \`allow-*-in-tests\` does NOT recognize custom macros as tests, so \`.unwrap()\` calls you add become NEW real \`unwrap_used\` errors. For any function under a non-\`#[test]\` attribute (or a plain non-test helper returning \`Result\`), keep the \`Result\` return type exactly as-is and replace \`.unwrap()\`/\`.expect()\` with \`?\` instead — propagate, don't panic.
- For \`clippy::exit\` (\`std::process::exit\`) reported OUTSIDE \`fn main\`: restructure so the function returns a value (an exit code, a \`Result\`, or \`std::process::ExitCode\`) and only \`main\` itself exits.

There are only THREE known cases where no rewrite exists and a scoped \`#[expect(clippy::lint_name, reason = "...")]\` (narrowest possible — the exact item, not the module) is acceptable. Use \`#[expect]\`, never \`#[allow]\`: this workspace denies \`clippy::allow_attributes\`, so an \`#[allow]\` is itself an error.
1. The lint fires on a derive macro's own generated code from an external crate (clippy's note says "this error originates in the derive macro '<Name>'") — there is no hand-written code to change.
2. \`clippy::uninhabited_references\` on an empty-enum \`match *self {}\` in a \`Display\`/\`Debug\` impl for an uninhabited error type — \`match self {}\` (without the deref) does not type-check because rustc considers \`&T\` always inhabited, so the deref is unavoidable.
3. A float-to-int cast (\`(x * 0.6) as u8\`): std has no non-\`as\` equivalent. Only the specific float-to-int line qualifies — do real \`try_from\` fixes for any int-to-int casts in the same function.
If you hit anything else you genuinely cannot fix without a suppression, do NOT add one — leave it unfixed and say exactly why in your summary so a human can look at it.

Never remove a \`use\` import just because it looks unused from a non-test read of the file — check whether it's referenced only inside \`#[cfg(test)] mod tests { ... }\` (via \`use super::*;\`) before deleting; removing a test-only import silently breaks \`cargo test\`/\`--all-targets\` even though the lib target still compiles.

Never change behavior. Do not reformat or touch code that isn't part of a listed error. Do not add tests, comments beyond what's specified above, or unrelated cleanup. Work quickly and do not explore the repo beyond this file.

When done, report {path, changed, summary}. For \`path\`, give ${multi ? 'the paths of all the files you edited, comma-separated' : 'the file path'}. For \`summary\`, one short sentence per distinct fix (or exactly which error you left unfixed and why, if any).`
}

// Bound one agent's workload by BOTH error count and prompt size. Overflow is
// deferred to the next round rather than dropped — a silent cap would read as
// "this file is clean" when it is not.
function budgetBlocks(file) {
  const blocks = []
  let chars = 0
  for (const b of file.errorBlocks) {
    if (blocks.length >= maxErrorsPerAgent) break
    if (blocks.length > 0 && chars + b.length > maxCharsPerAgent) break
    blocks.push(b)
    chars += b.length
  }
  return blocks
}

// Group files into agent-sized batches. A file is never split across two
// concurrent agents (see the note at the top of this file) — heavy files get a
// dedicated agent, light ones are packed together until the batch reaches
// issuesPerAgent or the prompt-size cap.
function planBatches(files) {
  const heavy = []
  const light = []
  for (const f of files) {
    ;(f.errorBlocks.length >= issuesPerAgent ? heavy : light).push(f)
  }

  const batches = heavy.map((f) => [{ file: f, blocks: budgetBlocks(f) }])

  let cur = []
  let n = 0
  let chars = 0
  for (const f of light) {
    const size = f.errorBlocks.length
    const cost = f.errorBlocks.reduce((c, b) => c + b.length, 0)
    if (n > 0 && (n + size > issuesPerAgent || chars + cost > maxCharsPerAgent)) {
      batches.push(cur)
      cur = []
      n = 0
      chars = 0
    }
    cur.push({ file: f, blocks: f.errorBlocks })
    n += size
    chars += cost
  }
  if (cur.length > 0) batches.push(cur)

  return batches
}

let round = 0
let prevTotal = Infinity
let discovery = await agent(discoverPrompt(), { schema: DISCOVER_SCHEMA, phase: 'Discover' })
log(`Initial: ${discovery.totalErrors} errors across ${discovery.files.length} files`)

while (discovery.files.length > 0 && round < maxRounds) {
  round++
  const roundLabel = `Fix round ${round}`
  const batches = planBatches(discovery.files)
  const thisRound = batches.reduce((n, b) => n + b.reduce((m, u) => m + u.blocks.length, 0), 0)
  const deferredTotal = batches.reduce(
    (n, b) => n + b.reduce((m, u) => m + (u.file.count - u.blocks.length), 0),
    0
  )
  log(
    `${roundLabel}: ${batches.length} fixer agents (${fixerModel}, no Bash) for ` +
      `${discovery.files.length} files, ${thisRound} errors this round` +
      (deferredTotal > 0 ? `, ${deferredTotal} deferred to a later round` : '')
  )

  await pipeline(batches, (batch) =>
    agent(fixPrompt(batch), {
      label:
        batch.length === 1
          ? `fix:${batch[0].file.path}`
          : `fix:${batch.length} files (${batch.reduce((n, u) => n + u.blocks.length, 0)} errors)`,
      phase: roundLabel,
      model: fixerModel,
      effort: 'low',
      agentType: 'clippy-fixer',
      schema: FIX_SCHEMA,
    })
  )

  discovery = await agent(discoverPrompt(), { schema: DISCOVER_SCHEMA, phase: 'Verify' })
  log(`After round ${round}: ${discovery.totalErrors} errors across ${discovery.files.length} files`)

  if (discovery.totalErrors >= prevTotal) {
    log(`No progress this round (${discovery.totalErrors} >= ${prevTotal}) — stopping early`)
    break
  }
  prevTotal = discovery.totalErrors
}

return {
  roundsRun: round,
  clean: discovery.totalErrors === 0,
  remainingErrors: discovery.totalErrors,
  remainingFiles: discovery.files.map((f) => ({ path: f.path, count: f.count })),
}
