# Tracey: Spec-to-Code Traceability

[bearcove/tracey](https://github.com/bearcove/tracey) — links specifications to
implementations and tests. Clone to `/tmp/tracey` for reference.

## What It Does

Tracey solves **specification-implementation-test drift**. It:

- Links markdown requirements (`r[auth.login]`) to code annotations (`// r[impl auth.login]`)
- Tracks coverage: which requirements have implementations and tests
- Detects stale references when specs are versioned and code points to old versions
- Provides reverse coverage: finds code that isn't justified by any spec

## Annotations

### In specs (markdown)

```markdown
r[auth.login]
Users must authenticate with username and password.

r[auth.login.mfa+2]
Multi-factor authentication is required for admin roles.
```

### In code (any language — 30+ supported via tree-sitter)

```rust
// r[impl auth.login]
fn login(username: &str, password: &str) -> Result<Token> { ... }

// r[verify auth.login]
#[test]
fn test_login() { ... }

// r[depends auth.login.mfa+2]
fn require_mfa(role: Role) -> bool { ... }
```

### Reference verbs

| Verb | Use | Meaning |
|------|-----|---------|
| `impl` | Production code | Implements the requirement |
| `verify` | Test code | Tests/validates the requirement |
| `depends` | Strict dependency | Must be re-reviewed if requirement changes |
| `related` | Loose reference | Related code shown during reviews |

## Configuration

`.config/tracey/config.styx`:

```styx
specs (
    {
        name signal-api
        include (docs/spec/**/*.md)
        impls (
            {
                name rust
                include (crates/signal/**/*.rs)
                exclude (target/**)
                test_include (crates/signal/**/tests/**/*.rs)
            }
        )
    }
)
```

## CLI Commands

```bash
tracey web --open              # Launch coverage dashboard
tracey query status            # Coverage overview
tracey query uncovered         # Requirements without implementations
tracey query untested          # Requirements without tests
tracey query stale             # Code pointing to old spec versions
tracey query unmapped          # Code without spec references
tracey query validate          # Find errors/warnings
tracey pre-commit              # Git hook — fail on stale/broken refs
tracey bump                    # Auto-bump versions when spec text changes
tracey lsp                     # Start LSP server (editor integration)
tracey mcp                     # Start MCP server (AI integration)
```

## Version Tracking

When a spec changes, bump its version:

```markdown
r[auth.login+2]
Users must authenticate with a valid token. (changed from password-based)
```

Code referencing v1 becomes **stale**:
```rust
// r[impl auth.login+1]  ← Stale! Spec is now v2
```

`tracey pre-commit` catches this before merge.

## Integration with AI

Tracey has MCP integration — AI assistants can query coverage, find
unimplemented requirements, and add annotations. Use `tracey ai` to
register the MCP server.

**Claude Code skill**: Use `/tracey` for interactive help with annotations,
finding requirements, and checking coverage within conversations.

## Best Practices

1. **Hierarchical IDs** — mirror spec structure: `signal.routing.parameter-mapping`
2. **Separate impl from verify** — use `test_include` to enforce test annotations stay in test files
3. **Version when behavior changes** — not when clarifying docs
4. **Pre-commit hook** — `tracey pre-commit` prevents stale references from merging
5. **CI validation** — `tracey query validate --deny warnings` as a CI gate
6. **Reverse coverage** — periodically check `tracey query unmapped` to find unspecified code

## Applying to FastTrackStudio

```rust
// crates/signal/signal-proto/src/services.rs
/// r[impl signal.profile.activate]
async fn activate(&self, profile_id: ProfileId, patch_id: Option<PatchId>) -> ...

// crates/signal/signal-controller/src/variation.rs
/// r[impl signal.variation.switch]
pub async fn switch_to_variation(&self, n: usize) -> SwitchResult { ... }

// crates/signal/signal/tests/signal_live_runtime.rs
/// r[verify signal.profile.activate]
#[test]
async fn test_profile_activation() { ... }
```
