export const meta = {
  name: 'clippy-pedantic-migration',
  description: 'Iteratively fix cargo clippy pedantic/nursery/panic-lint denials using fast fixer agents',
  whenToUse: 'A Rust repo already has (or just got) a strict clippy config (pedantic+nursery deny, unwrap/expect/indexing/arithmetic denies) and cargo clippy now fails with many errors that need fixing file-by-file.',
  phases: [
    { title: 'Discover', detail: 'run cargo clippy, group errors by file' },
    { title: 'Fix', detail: 'one fast agent per file, edits only' },
    { title: 'Verify', detail: 're-run clippy, loop until clean or stalled' },
  ],
}

// args: {
//   cwd: string (required) — absolute path to the repo root to run cargo in
//   package: string | null — cargo -p target; null/omitted = whole workspace
//   extraClippyArgs: string — appended to the cargo clippy invocation (default '')
//   fixerModel: 'haiku' | 'sonnet' | 'opus' | 'fable' (default 'haiku')
//   maxRounds: number (default 4)
// }

const cwd = args.cwd
if (!cwd) throw new Error('args.cwd is required — absolute path to the repo root')
const pkgFlag = args.package
  ? `-p ${args.package} --no-deps --all-targets --keep-going`
  : '--workspace --no-deps --all-targets --keep-going'
const extra = args.extraClippyArgs || ''
const fixerModel = args.fixerModel || 'haiku'
const maxRounds = args.maxRounds || 4

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
          errors: { type: 'string' },
        },
        required: ['path', 'count', 'errors'],
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
2. Run: timeout 900 cargo clippy ${pkgFlag} ${extra} > /tmp/clippy_migration_out.log 2>&1 ; true
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
    path = m.group(1)
    by_file.setdefault(path, []).append(b.strip())
files = [{"path": p, "count": len(v), "errors": "\\n\\n".join(v)} for p, v in by_file.items()]
files.sort(key=lambda f: -f["count"])
total = sum(f["count"] for f in files)
print(json.dumps({"totalErrors": total, "files": files}))

4. Report back the exact JSON object the Python script printed (totalErrors, files[] with path/count/errors), matching the required schema exactly. Do not summarize or truncate the "errors" field — it must contain the full raw clippy error blocks for that file, verbatim, so a fixer agent can act on them without re-running clippy.`
}

function fixPrompt(file) {
  return `You are fixing cargo clippy lint violations in exactly one file. Do NOT run cargo, clippy, or any build/test command — another stage verifies your work later, and parallel cargo invocations would fight over a locked build directory. Only use Read and Edit (or Write) on this one file.

Repo root: ${cwd}
File: ${file.path} (path is relative to the repo root above)

Here are the exact clippy errors reported for this file (line numbers refer to the file's current state on disk):

${file.errors}

Fix every one of them by editing the file directly with a REAL fix. Never use \`#[allow(clippy::...)]\` (item, function, module, or file level) to satisfy a lint — not even when the flagged code looks safe/bounded. Always rewrite instead:
- \`checked_add\`/\`checked_sub\`/\`saturating_add\`/\`saturating_sub\`/\`saturating_neg\`/\`saturating_mul\` instead of raw +, -, *, unary - on ints.
- \`.get(i)\`/\`.get_mut(i)\`/\`.get(a..b)\` (with \`.unwrap_or(...)\`, \`?\`, or an \`else\` branch) instead of \`v[i]\`/\`&v[a..b]\` indexing/slicing — including on \`&str\`, where \`.get()\` returns \`Option<&str>\`.
- \`i32::try_from(x).unwrap_or(FALLBACK)\` (a real, non-panicking fallback value, never \`.unwrap()\`) instead of \`x as i32\`-style casts. For an enum-to-integer cast on a small fieldless enum, prefer an explicit \`match\` returning the integer per variant over \`as\`.
- \`Option::map_or\`/\`map_or_else\` instead of \`if let Some(x) = opt {..} else {..}\`.
- Add missing \`# Errors\`/\`# Panics\` doc sections truthfully (state what actually causes the error/panic).
- \`const fn\` where clippy suggests it and it's legal (all called functions must also be const).
- \`x.clone_from(&y)\` instead of \`x = y.clone()\`.
- For \`too_many_lines\`: actually split the function into smaller named helpers (or, for a big literal data table, reformat entries onto fewer lines) — don't just suppress.
- For \`panic_in_result_fn\` inside a \`#[test]\`/\`#[tokio::test]\` function: change the function to NOT return \`Result\` (return \`()\`) and \`.unwrap()\` the fallible setup calls instead — \`.unwrap()\`/\`.expect()\` ARE allowed inside a function directly marked \`#[test]\`/\`#[tokio::test]\`/\`#[cfg(test)]\`, just not one that also returns \`Result\` and uses \`assert!\`/\`assert_eq!\` (those still count as "panic in a Result fn"). **STOP — do NOT do this if the function is annotated with any OTHER attribute** (a custom macro like \`#[reaper_test]\`, or anything not literally \`#[test]\`/\`#[tokio::test]\`/\`#[rstest]\`) **or if its signature was already \`-> Result<(), SomeErrorType>\` before you started.** A custom test-harness macro typically wraps the function and requires that exact \`Result\`-returning signature — stripping it breaks the macro with a confusing \`Pin<Box<dyn Future<Output = Result<...>>>>\` type-mismatch error, and clippy's \`allow-*-in-tests\` does NOT recognize custom macros as tests, so \`.unwrap()\` calls you add will themselves become NEW real \`unwrap_used\` errors. For any function under a non-\`#[test]\`/non-\`#[tokio::test]\` attribute (or a plain non-test helper function returning \`Result\`), keep the \`Result\` return type exactly as-is and replace \`.unwrap()\`/\`.expect()\` with \`?\` instead (adding \`Ok(...)\`/\`Ok(())\` where needed) — propagate, don't panic.
- For \`clippy::exit\` (\`std::process::exit\`) reported OUTSIDE \`fn main\`: this fires because the call isn't in \`main\` (clippy allows it directly in \`main\`) — restructure so the function returns a value (an exit code, a \`Result\`, or \`std::process::ExitCode\`) and only \`main\` itself calls \`std::process::exit\`/returns \`ExitCode\`, rather than adding an allow.

There are only TWO known cases where no rewrite exists and a scoped \`#[allow(clippy::lint_name)]\` (narrowest possible — the exact item, not the module) plus a one-line comment is acceptable:
1. The lint fires on a derive macro's own generated code from an external crate (clippy's note says "this error originates in the derive macro '<Name>'") — there is no hand-written code to change.
2. \`clippy::uninhabited_references\` on an empty-enum \`match *self {}\` in a \`Display\`/\`Debug\` impl for an uninhabited error type — \`match self {}\` (without the deref) does not type-check because rustc considers \`&T\` always inhabited, so the deref (and therefore the lint) is unavoidable.
If you hit anything else you genuinely cannot fix without an allow, do NOT add one — leave it unfixed and say exactly why in your summary so a human can look at it.

Never remove a \`use\` import just because it looks unused from a non-test read of the file — check whether it's referenced only inside \`#[cfg(test)] mod tests { ... }\` (via \`use super::*;\`) before deleting; removing a test-only import silently breaks \`cargo test\`/\`--all-targets\` even though the lib target still compiles.

Never change behavior. Do not reformat or touch code that isn't part of a listed error. Do not add tests, comments beyond what's specified above, or unrelated cleanup.
If a fix requires changing a public function's signature (e.g. \`&(impl ToString + ?Sized)\` instead of \`impl ToString\` to fix \`needless_pass_by_value\`) and you can see call sites in the given error text or via a quick Read of files you already have context on, update them too; if you can't be sure who else calls it, make the least invasive real fix you can rather than reaching for an allow.

When done, report {path, changed, summary} — summary is one short sentence per distinct fix you made (or exactly which error you left unfixed and why, if any).`
}

let round = 0
let prevTotal = Infinity
let discovery = await agent(discoverPrompt(), { schema: DISCOVER_SCHEMA, phase: 'Discover' })
log(`Initial: ${discovery.totalErrors} errors across ${discovery.files.length} files`)

while (discovery.files.length > 0 && round < maxRounds) {
  round++
  const roundLabel = `Fix round ${round}`
  log(`${roundLabel}: dispatching ${discovery.files.length} fixer agents (${fixerModel})`)

  await pipeline(
    discovery.files,
    (file) =>
      agent(fixPrompt(file), {
        label: `fix:${file.path}`,
        phase: roundLabel,
        model: fixerModel,
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
