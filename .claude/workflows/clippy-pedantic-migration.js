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
//   issuesPerAgent: number (default 10) — bin-packing target. Files with fewer
//     errors than this are grouped so one agent fixes ~this many issues across
//     several small files, instead of one agent per file.
// }
//
// SCOPE: the fixers only ever see the MECHANICAL lint set (see the discover
// script). Judgment lints in DSP code — indexing_slicing, as_conversions,
// arithmetic_side_effects, cast_*, suboptimal_flops — are counted and reported
// but never auto-fixed, because the correct rewrite depends on whether the code
// sits on an audio callback. A cheap fixer cannot know that and refactors the
// API instead. So a "clean" result from this workflow means "clean of the
// mechanical set", NOT clean of the gate. Read withheldForHumanReview.
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
// Discovery MUST run with lints capped to warn. Under `deny` a crate aborts
// partway through, so the visible error set depends on which errors were just
// fixed — measured across four rounds it went 3954 -> 3892 -> 1655 -> 684 ->
// 2574, and rounds saw 11-13 of 49 files. The Cargo.toml gate stays `deny`;
// only this measurement pass is capped.
const extra = `${args.extraClippyArgs || ''} -- --cap-lints warn`.trim()
const fixerModel = args.fixerModel || 'haiku'
const maxRounds = args.maxRounds || 4
const maxErrorsPerAgent = args.maxErrorsPerAgent || 60
const issuesPerAgent = args.issuesPerAgent || 10

const DISCOVER_SCHEMA = {
  type: 'object',
  properties: {
    totalErrors: { type: 'number' },
    withheld: { type: 'object' },
    compileErrors: { type: 'array', items: { type: 'string' } },
    files: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          path: { type: 'string' },
          count: { type: 'number' },
          errorFile: { type: 'string' },
        },
        required: ['path', 'count', 'errorFile'],
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
3. Run this Python script (via \`python3 - <<'PY' ... PY\`) EXACTLY as written. It
   writes each file's clippy errors to its own text file on disk and prints a
   small manifest. Print ONLY the manifest JSON to stdout:

import re, json, os, hashlib, shutil
# Lints whose only possible fix is reshaping an API or renaming domain
# variables. A fixer agent cannot compile, so it cannot verify such a change is
# safe — and twice now one has grouped a struct's pub fields and broken the
# crate for everyone. These stay DENIED in Cargo.toml (the gate is honest);
# they are simply withheld from the automated fixers and reported for a human.
# Only these lints are handed to the automated fixers. Everything else is
# REPORTED, never touched. Two categories are deliberately excluded:
#   - judgment lints in DSP code (indexing_slicing, as_conversions,
#     arithmetic_side_effects, cast_*, suboptimal_flops): the right fix depends
#     on whether the code is on an audio callback, which a cheap fixer cannot
#     know. It reaches for an API refactor instead. Those go to a human.
#   - missing_const_for_fn: fires only in the build where a cfg-gated body is
#     the inert branch, so blanket-adding const breaks the other feature.
MECHANICAL = {
    "unreadable_literal", "doc_markdown", "must_use_candidate", "use_self",
    "uninlined_format_args", "redundant_closure_for_method_calls",
    "explicit_iter_loop", "ignored_unit_patterns", "semicolon_if_nothing_returned",
    "redundant_pub_crate", "derive_partial_eq_without_eq", "map_unwrap_or",
    "single_match_else", "too_long_first_doc_paragraph", "missing_errors_doc",
    "missing_panics_doc", "format_push_string", "return_self_not_must_use",
    "needless_for_each", "match_same_arms", "unnested_or_patterns",
    "manual_let_else", "redundant_else", "doc_link_with_quotes",
    # NOT unused_self: it turns &self methods into associated functions, which
    # breaks every self.foo() call site in other files. Broke signal-sampler
    # on the first workspace run.
}
OUT = "/tmp/clippy_migration_errors"
shutil.rmtree(OUT, ignore_errors=True)
os.makedirs(OUT, exist_ok=True)
with open("/tmp/clippy_migration_out.log") as f:
    log = f.read()
blocks = re.split(r'\\n(?=(?:error|warning)(?:\\[|:))', log)
by_file = {}
withheld = {}
compile_errors = []
for b in blocks:
    if not (b.startswith("error") or b.startswith("warning")):
        continue
    m = re.search(r'--> (\\S+):(\\d+):(\\d+)', b)
    if not m:
        continue
    ln = re.search(r'index\\.html#([a-z_]+)', b)
    if ln is None:
        # Not a clippy lint. If it is an ERROR it is a genuine compile failure —
        # a fixer corrupted the source. Count it; the workflow aborts on these.
        # (Discovery caps lints to warn, so anything still at error level is real.)
        if b.startswith("error"):
            compile_errors.append(b.strip()[:400])
        continue
    if ln.group(1) not in MECHANICAL:
        withheld.setdefault(ln.group(1), 0)
        withheld[ln.group(1)] += 1
        continue
    by_file.setdefault(m.group(1), []).append(b.strip())
files = []
for p, v in by_file.items():
    name = hashlib.sha1(p.encode()).hexdigest()[:16] + ".txt"
    dest = os.path.join(OUT, name)
    with open(dest, "w") as fh:
        fh.write(("\\n\\n" + "-" * 60 + "\\n\\n").join(v))
    files.append({"path": p, "count": len(v), "errorFile": dest})
files.sort(key=lambda f: -f["count"])
print(json.dumps({"totalErrors": sum(f["count"] for f in files), "files": files, "withheld": withheld, "compileErrors": compile_errors}))

4. Report back the exact JSON object the script printed: totalErrors, and files[]
   with path/count/errorFile. This manifest is small by design — the error text
   stays on disk and the fixer agents read it themselves. Do NOT inline, quote,
   or summarize any error text into your answer.`
}

function fixPrompt(units) {
  const multi = units.length > 1
  const total = units.reduce((n, u) => n + u.take, 0)
  const header = multi
    ? `You are fixing ${total} cargo clippy lint violations spread across ${units.length} files. Only use Read and Edit (or Write), and only on the files listed below. You have no Bash tool: verification is a later stage's job.

Repo root: ${cwd}

Work through the files one at a time, in the order given. They are independent — no other agent is touching any of them.`
    : `You are fixing cargo clippy lint violations in exactly one file. Only use Read and Edit (or Write) on this one file. You have no Bash tool: verification is a later stage's job.

Repo root: ${cwd}`

  const body = units
    .map((u) => {
      const partial = u.take < u.file.count
      return `${multi ? '========================================\n' : ''}File: ${u.file.path} (relative to the repo root above)
Its exact clippy errors: READ THE FILE ${u.file.errorFile} — it holds ${u.file.count} error block${u.file.count === 1 ? '' : 's'}, separated by dashed lines. Line numbers in it refer to the source file's current state on disk.${
        partial
          ? `\nThis file has more errors than one pass should take: fix ONLY THE FIRST ${u.take} blocks in that error file. The remaining ${u.file.count - u.take} go to a later round — leave them alone.`
          : ''
      }`
    })
    .join('\n\n')

  return `${header}

${body}

Read each error file first, then fix the source. The error text is on disk rather than in this prompt so that nothing is truncated — if an error file seems short or cut off, say so in your summary rather than guessing at what it said.

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

NEVER change a struct definition, enum definition, function signature, trait, or any other public API shape. This is the single most damaging thing a fixer can do here and it has already happened once: an agent "fixed" a bools-heavy struct by grouping three \`pub\` fields into a new config struct, did not update every call site, and broke the crate's compilation for everyone. If a lint (\`struct_excessive_bools\`, \`fn_params_excessive_bools\`, \`too_many_arguments\`, \`needless_pass_by_value\` on a public item, and the like) can only be satisfied by reshaping an API, LEAVE IT UNFIXED and say so in your summary. A remaining lint is cheap; a broken build that another agent has to bisect is not. You cannot compile, so you cannot possibly verify such a change is safe.

Leave variable names alone in numerical/DSP code. \`a\`, \`b\`, \`q\`, \`w0\`, \`l\`, \`r\` are the standard names from filter cookbooks, and renaming them to satisfy \`similar_names\`/\`many_single_char_names\` makes the math harder to check against its reference. Report those as unfixed instead.

Never change behavior. Do not reformat or touch code that isn't part of a listed error. Do not add tests, comments beyond what's specified above, or unrelated cleanup. Work quickly and do not explore the repo beyond this file.

When done, report {path, changed, summary}. For \`path\`, give ${multi ? 'the paths of all the files you edited, comma-separated' : 'the file path'}. For \`summary\`, one short sentence per distinct fix (or exactly which error you left unfixed and why, if any).`
}

// Bound one agent's workload by BOTH error count and prompt size. Overflow is
// deferred to the next round rather than dropped — a silent cap would read as
// "this file is clean" when it is not.
// Group files into agent-sized batches. A file is never split across two
// concurrent agents (see the note at the top of this file) — heavy files get a
// dedicated agent, light ones are packed together until the batch reaches
// issuesPerAgent. `take` is how many of that file's errors this round fixes;
// the rest are deferred to a later round, never dropped.
function planBatches(files) {
  const heavy = []
  const light = []
  for (const f of files) {
    ;(f.count >= issuesPerAgent ? heavy : light).push(f)
  }

  const batches = heavy.map((f) => [{ file: f, take: Math.min(f.count, maxErrorsPerAgent) }])

  let cur = []
  let n = 0
  for (const f of light) {
    if (n > 0 && n + f.count > issuesPerAgent) {
      batches.push(cur)
      cur = []
      n = 0
    }
    cur.push({ file: f, take: f.count })
    n += f.count
  }
  if (cur.length > 0) batches.push(cur)

  return batches
}

let round = 0
let prevTotal = Infinity
let discovery = await agent(discoverPrompt(), { schema: DISCOVER_SCHEMA, phase: 'Discover' })
function withheldNote(d) {
  const w = d.withheld || {}
  const keys = Object.keys(w)
  if (keys.length === 0) return ''
  const n = keys.reduce((s, k) => s + w[k], 0)
  return ` | ${n} left for human review (${keys.map((k) => `${k}:${w[k]}`).join(', ')})`
}

log(`Initial: ${discovery.totalErrors} errors across ${discovery.files.length} files${withheldNote(discovery)}`)

function assertCompiles(d, when) {
  const errs = d.compileErrors || []
  if (errs.length === 0) return
  log(`ABORTING: ${errs.length} compile error(s) ${when}. A fixer corrupted the source.`)
  for (const e of errs.slice(0, 5)) log(`  ${e.split('\n')[0]}`)
  throw new Error(
    `Build is broken ${when} (${errs.length} compile errors) — stopping. ` +
      `Revert the offending files (git checkout HEAD -- <path>) before re-running. ` +
      `Fixing lints on a crate that does not compile produces garbage.`
  )
}

assertCompiles(discovery, 'before the first round')

while (discovery.files.length > 0 && round < maxRounds) {
  round++
  const roundLabel = `Fix round ${round}`
  const batches = planBatches(discovery.files)
  const thisRound = batches.reduce((n, b) => n + b.reduce((m, u) => m + u.take, 0), 0)
  const deferredTotal = batches.reduce(
    (n, b) => n + b.reduce((m, u) => m + (u.file.count - u.take), 0),
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
          : `fix:${batch.length} files (${batch.reduce((n, u) => n + u.take, 0)} errors)`,
      phase: roundLabel,
      model: fixerModel,
      effort: 'low',
      agentType: 'clippy-fixer',
      schema: FIX_SCHEMA,
    })
  )

  discovery = await agent(discoverPrompt(), { schema: DISCOVER_SCHEMA, phase: 'Verify' })
  log(`After round ${round}: ${discovery.totalErrors} errors across ${discovery.files.length} files${withheldNote(discovery)}`)
  assertCompiles(discovery, `after round ${round}`)

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
  withheldForHumanReview: discovery.withheld || {},
}
