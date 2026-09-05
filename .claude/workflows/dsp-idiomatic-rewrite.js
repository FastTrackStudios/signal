export const meta = {
  name: 'dsp-idiomatic-rewrite',
  description: 'Rewrite a DSP crate into idiomatic Rust against bit-exact golden vectors, fixing clippy judgment lints in parallel',
  phases: [
    { title: 'Discover', detail: 'clippy at warn, grouped per file, error text to disk' },
    { title: 'Fix', detail: 'haiku fixers, one file each, edits only, no Bash' },
    { title: 'Verify', detail: 'compile + lint + tests + goldens unchanged; abort on drift' },
  ],
}

// args: { cwd, package, recipe, maxRounds, issuesPerAgent, maxErrorsPerAgent, fixerModel }
//
// HOW THIS DIFFERS from clippy-pedantic-migration, and why:
//
// That workflow WITHHOLDS the judgment lints — indexing_slicing, as_conversions,
// arithmetic_side_effects, cast_* — because "the correct rewrite depends on
// whether the code sits on an audio callback, which a cheap fixer cannot know".
// That was the right call when it was written. It is not any more, for two
// reasons:
//
//   1. There is now a recipe (args.recipe) giving each of those lints exactly
//      one correct answer in this codebase, so the fixer is applying a decision
//      rather than making one.
//   2. There are now bit-exact golden vectors. A fixer that changes the audio
//      is caught by the Verify phase in the same round, not six months later by
//      ear. That is the thing that was actually missing.
//
// So this workflow hands the judgment lints to the fixers, and takes on the
// obligation of checking them properly. Verify is therefore NOT optional and
// NOT advisory: it compiles, lints, runs the tests, AND diffs the golden
// directory, and the loop STOPS on any of those failing rather than carrying a
// broken tree into another round.
//
// The API-shape lints stay withheld regardless. A fixer cannot compile, so it
// cannot verify that reshaping a struct is safe, and doing it anyway has broken
// this repo twice.

const cwd = args.cwd
if (!cwd) throw new Error('args.cwd is required — absolute path to the repo root')
const pkg = args.package
if (!pkg) throw new Error('args.package is required — the crate to rewrite')
const recipe = args.recipe || 'docs/agents/dsp-lint-recipe.md'
const maxRounds = args.maxRounds || 4
const issuesPerAgent = args.issuesPerAgent || 25
const maxErrorsPerAgent = args.maxErrorsPerAgent || 70
const fixerModel = args.fixerModel || 'haiku'

const goldenDir = `features/**/${pkg}/tests/golden`

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

const VERIFY_SCHEMA = {
  type: 'object',
  properties: {
    compiles: { type: 'boolean' },
    testsPass: { type: 'boolean' },
    goldensUnchanged: { type: 'boolean' },
    remainingLints: { type: 'number' },
    detail: { type: 'string' },
  },
  required: ['compiles', 'testsPass', 'goldensUnchanged', 'remainingLints', 'detail'],
}

function discoverPrompt() {
  return `Run this from ${cwd} and report the result as JSON.

1. cd ${cwd}
2. Run: timeout 1800 cargo clippy -p ${pkg} --no-deps --all-targets --keep-going -- --cap-lints warn > /tmp/dsp_rewrite_out.log 2>&1 ; true
   (Lints MUST be capped to warn here. Under \`deny\` a crate aborts partway, so
   the visible error set depends on which errors were just fixed and the count
   swings wildly between rounds. The Cargo.toml gate stays deny.)
   If another cargo process holds the build lock this sits silently; that is
   expected, do not start a second cargo command.
3. Run this Python script EXACTLY as written via \`python3 - <<'PY' ... PY\`. It
   writes each file's errors to its own file on disk and prints a small
   manifest. Print ONLY that JSON to stdout.

import re, json, os, hashlib, shutil
# Withheld: the only fix is reshaping an API, which a fixer cannot verify
# because it cannot compile. Reported for a human, never auto-fixed.
WITHHELD = {
    "struct_excessive_bools", "fn_params_excessive_bools", "too_many_arguments",
    "needless_pass_by_value", "unused_self", "module_name_repetitions",
    "similar_names", "many_single_char_names",
}
OUT = "/tmp/dsp_rewrite_errors"
shutil.rmtree(OUT, ignore_errors=True)
os.makedirs(OUT, exist_ok=True)
log = open("/tmp/dsp_rewrite_out.log").read()
blocks = re.split(r'\\n(?=(?:error|warning)(?:\\[|:))', log)
by_file, withheld, compile_errors = {}, {}, []
for b in blocks:
    if not (b.startswith("error") or b.startswith("warning")):
        continue
    m = re.search(r'--> (\\S+):(\\d+):(\\d+)', b)
    if not m:
        continue
    ln = re.search(r'index\\.html#([a-z_]+)', b)
    if ln is None:
        if b.startswith("error"):
            compile_errors.append(b.strip()[:400])
        continue
    if ln.group(1) in WITHHELD:
        withheld[ln.group(1)] = withheld.get(ln.group(1), 0) + 1
        continue
    by_file.setdefault(m.group(1), []).append(b.strip())
files = []
for p, v in by_file.items():
    name = hashlib.sha1(p.encode()).hexdigest()[:16] + ".txt"
    dest = os.path.join(OUT, name)
    open(dest, "w").write(("\\n\\n" + "-" * 60 + "\\n\\n").join(v))
    files.append({"path": p, "count": len(v), "errorFile": dest})
files.sort(key=lambda f: -f["count"])
print(json.dumps({"totalErrors": sum(f["count"] for f in files), "files": files,
                  "withheld": withheld, "compileErrors": compile_errors}))

4. Report the exact JSON the script printed. Do NOT inline, quote or summarize
   any error text into your answer — it stays on disk and the fixers read it.`
}

function fixPrompt(units) {
  const multi = units.length > 1
  const total = units.reduce((n, u) => n + u.take, 0)
  const body = units
    .map((u) => {
      const partial = u.take < u.file.count
      return `${multi ? '========================================\n' : ''}File: ${u.file.path} (relative to ${cwd})
Its exact clippy errors: READ THE FILE ${u.file.errorFile} — ${u.file.count} error block${u.file.count === 1 ? '' : 's'}, separated by dashed lines. Line numbers refer to the file's current state on disk.${
        partial
          ? `\nFix ONLY THE FIRST ${u.take} blocks in that error file. The remaining ${u.file.count - u.take} go to a later round — leave them alone.`
          : ''
      }`
    })
    .join('\n\n')

  return `You are converting a realtime DSP crate to idiomatic Rust. ${total} clippy findings across ${units.length} file${units.length === 1 ? '' : 's'}.

Repo root: ${cwd}

**FIRST, read ${cwd}/${recipe}.** It is short, and it gives each of these lints
exactly one correct answer in this codebase. Follow it literally. Do not invent
a different fix for a lint the recipe covers — several plausible-looking
alternatives (a \`.unwrap_or(0.0)\` fallback on an audio path, a clamp that
changes which channel state gets used) compile, satisfy the lint, and change
the audio.

**The one rule: never change what the code computes.** This crate has bit-exact
reference vectors. A rewrite that moves a single sample fails the build after
you finish, and it will be traced back to your file. When two fixes are both
valid, take the one that is obviously arithmetic-preserving.

${body}

Read each error file first, then fix the source. If an error file looks
truncated, say so in your summary rather than guessing.

You have no Bash tool and that is deliberate — one workspace means one build
lock, and a fixer running cargo idles behind the orchestrator for half an hour
looking exactly like a hang. Verification is a later stage's job.

Hard limits, each of which has cost this project real time:
- NEVER reshape a struct, enum, function signature, or trait. If a lint can
  only be satisfied that way, leave it and say so in your summary.
- NEVER use \`#[allow(...)]\`; this workspace denies \`clippy::allow_attributes\`.
  A genuinely necessary suppression is \`#[expect(clippy::lint, reason = "...")]\`
  on the narrowest item.
- NEVER rename variables in numerical code. \`a\`, \`b\`, \`q\`, \`w0\`, \`l\`, \`r\` are
  cookbook names; renaming makes the math uncheckable against its reference.
- NEVER delete a \`use\` that looks unused — check \`#[cfg(test)] mod tests\` first.
- Do not add tests, do not reformat unrelated code, do not explore the repo
  beyond the files listed and the recipe.

When done report {path, changed, summary}: ${multi ? 'all edited paths, comma-separated' : 'the file path'}, and one short sentence per distinct fix — or exactly which errors you left alone and why.`
}

function verifyPrompt() {
  return `Verify the in-progress DSP rewrite of \`${pkg}\`. Run these from ${cwd}, in order, and report JSON.

1. cd ${cwd}
2. \`cargo check -p ${pkg} --all-targets --message-format=short 2>&1 | tail -40\`
   Any line starting with \`error\` means a fixer corrupted the source.
   Set compiles=false and put the first few errors verbatim in \`detail\`. STOP HERE if so.
3. \`cargo clippy -p ${pkg} --all-targets --message-format=short 2>&1 | grep -cE '^[a-z].*error' || true\`
   That count is \`remainingLints\`.
4. \`cargo nextest run -p ${pkg} --no-fail-fast 2>&1 | tail -30\`
   testsPass = the summary line reports 0 failed.
5. \`git status --porcelain | grep -E 'tests/golden/' || echo CLEAN\`
   goldensUnchanged = the output is exactly \`CLEAN\`.

**Step 5 is the one that matters.** The reference vectors were recorded on the
unmodified code; a refactor that is genuinely behaviour-preserving leaves every
one byte-identical. If any golden file shows as modified, the rewrite changed
the audio: set goldensUnchanged=false and list the changed files in \`detail\`.
Do NOT re-record them, do not run anything with UPDATE_GOLDEN set, and do not
try to fix it — report and stop.

If tests fail, put the failing test names and the assertion text in \`detail\`.
Report only the JSON.`
}

function planBatches(files) {
  const heavy = []
  const light = []
  for (const f of files) (f.count >= issuesPerAgent ? heavy : light).push(f)

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

let discovery = await agent(discoverPrompt(), { schema: DISCOVER_SCHEMA, phase: 'Discover' })
const started = discovery.totalErrors
const log = []

for (let round = 1; round <= maxRounds; round++) {
  if (!discovery.files || discovery.files.length === 0) break
  if ((discovery.compileErrors || []).length > 0) {
    log.push(`round ${round}: ABORTED before fixing — tree does not compile: ${discovery.compileErrors[0]}`)
    break
  }

  const batches = planBatches(discovery.files)
  await parallel(
    batches.map((units) => () =>
      agent(fixPrompt(units), {
        label: `fix:${units.map((u) => u.file.path.split('/').pop()).join(',').slice(0, 40)}`,
        phase: 'Fix',
        subagent_type: 'clippy-fixer',
        model: fixerModel,
      })
    )
  )

  const verdict = await agent(verifyPrompt(), { schema: VERIFY_SCHEMA, phase: 'Verify' })
  log.push(
    `round ${round}: ${discovery.totalErrors} -> ${verdict.remainingLints} lints, ` +
      `compiles=${verdict.compiles} tests=${verdict.testsPass} goldens=${verdict.goldensUnchanged}`
  )

  // Any of these means the tree is worse than when the round started, and
  // another round of edits on top would only bury the cause.
  if (!verdict.compiles || !verdict.testsPass || !verdict.goldensUnchanged) {
    return {
      package: pkg,
      stoppedAt: `round ${round}`,
      reason: !verdict.compiles
        ? 'the crate no longer compiles'
        : !verdict.goldensUnchanged
          ? 'the rewrite CHANGED THE AUDIO — golden vectors differ'
          : 'tests failed',
      detail: verdict.detail,
      log,
      advice:
        'Do not re-record the goldens. `git diff` the source since the last commit, ' +
        'find the file whose change was not arithmetic-preserving, and revert that file.',
    }
  }

  if (verdict.remainingLints === 0) {
    return { package: pkg, started, remaining: 0, rounds: round, log, withheld: discovery.withheld }
  }

  discovery = await agent(discoverPrompt(), { schema: DISCOVER_SCHEMA, phase: 'Discover' })
}

return {
  package: pkg,
  started,
  remaining: discovery.totalErrors,
  log,
  withheld: discovery.withheld,
  note: 'Withheld lints are API-shape changes a fixer cannot verify; they need a human.',
}
