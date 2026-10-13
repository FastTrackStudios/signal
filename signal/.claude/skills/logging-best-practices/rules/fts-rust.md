# Wide events in this repo (Rust / architect / Task)

Project-specific companion to `SKILL.md`. The upstream skill is written
in TypeScript; this is how the same pattern is realised here. Read both.

## The span IS the wide event

Do **not** build a parallel `HashMap` and log it at the end. The
request-scoped container already exists and already propagates across
await points:

- **vox RPC** — `architect`'s `LayerRouter::call_span` opens exactly one
  span per dispatched call (`libs/architect/architect/src/layer.rs`),
  named `Svc/method` via `otel.name`, carrying `rpc.service`,
  `rpc.method`, `rpc.scope`.
- **HTTP** — `tower_http::trace::TraceLayer` in
  `apps/task/server/src/main.rs`, carrying method + MATCHED route.

Both are exported to Tempo, so enriching the span gives you the trace id,
the timing, and the parent/child structure for free. This is the
article's own preferred end state: *"Ideally, your wide events ARE your
trace spans."*

## Adding fields

`tracing`'s macros take a **static** field list — you cannot add a field
the macro did not declare. That is what `architect_telemetry::wide` is for:

```rust
use architect_telemetry::wide;

wide::set("auth.principal_kind", "anonymous");
wide::set("auth.user_id", user_id.clone());
wide::set_display("perm.resource", &resource);  // any Display
```

It writes a dynamic attribute onto the current OTel span, and is a no-op
when no OTel layer is installed — so call it from library code without
gating the call site.

## Where to enrich

Prefer a **decorator on an existing seam** over editing framework crates.
`architect` and `architect-auth` stay telemetry-agnostic; Task wraps them:

- `permits::AuditedIdentityResolver` wraps `SessionIdentityResolver` →
  `auth.*`, `org.slug`
- `permits::GateAudit` implements `AuditSink` → `perm.*`

That is the pattern to copy for a new dimension: find the trait the
framework already calls per request, wrap it, set fields.

## Field names in use

Keep these stable — renaming a field breaks every saved query.

| Field | Values |
|---|---|
| `rpc.service` / `rpc.method` | e.g. `TimerService` / `list_sessions` |
| `rpc.scope` | instance scope, `""` when unscoped |
| `org.slug` | high cardinality — keep it |
| `auth.principal_kind` | `user` \| `anonymous` |
| `auth.user_id` | present when resolved |
| `auth.token_presented` | bool |
| `auth.outcome` | `resolved` \| `rejected` \| `absent` |
| `perm.decision` | `allow` \| `deny` \| `would_deny` |
| `perm.mode` | `enforcing` \| `observe-only` |
| `perm.principal` / `perm.resource` / `perm.action` / `perm.reason` | |

`auth.outcome` earns its place: `rejected` (token sent, session store
said no) and `absent` (client never sent one) are the *same*
`Principal::Anonymous`, and telling them apart is the difference between
"sessions are expiring" and "the UI never signed in".

## Hard rules

- **Never** put a token, password, or session id in a field. These are
  exported off-box. Record the *shape* (`token_presented: true`), not the
  secret.
- **Never** put a raw URI or note path in a field — org slugs and note
  paths leak, and the cardinality is unbounded in the bad way. Use the
  MATCHED route; that rule predates this skill and still holds.
- Allowed/successful outcomes ride the wide event **only**. A log line
  per allow is the scatter this pattern exists to delete. Denials keep
  one line, because they are alertable.

## Querying

Tempo (TraceQL), via Grafana → Explore:

```
{ resource.service.name = "task-server" && span.auth.outcome = "rejected" }
{ span.perm.decision = "would_deny" && span.org.slug = "fasttrackstudios" }
{ span.rpc.service = "TimerService" && duration > 500ms }
```

Loki, for the log records that carry the same trace id:

```
{service_name="task-server"} | json | perm_decision="deny"
```

## Debugging is not printing

The `eprintln!` reflex is the anti-pattern this skill exists to
delete, and it applies to *scaffolding* too, not just committed code:

1. **Reproduce in a failing unit test.** A test is the artifact that
   outlives the session — it pins the bug, proves the fix, and guards
   the regression. `eprintln!` output proves nothing once the terminal
   scrolls.
2. **Query the span.** If the code already runs under a request span,
   the answer is usually a missing `wide::set` field away — add the
   field, keep it, and read it in Tempo/`--no-capture` test output.
   That field is *useful forever*; a print statement is noise you must
   remember to remove.
3. **Live process, last resort:** `tracing::debug!` with structured
   fields behind `RUST_LOG=target=debug`, and delete it before
   committing. Never `println!`/`eprintln!`/`dbg!` — they bypass the
   subscriber, can't be filtered, and end up committed.

In tests themselves, assertions with rich messages
(`assert!(cond, "context: {value}")`) replace prints — a failing
assertion shows its context exactly when needed.
