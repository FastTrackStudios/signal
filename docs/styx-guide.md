# Styx Configuration Format

[bearcove/styx](https://github.com/bearcove/styx) — a clean configuration format.
Clone to `/tmp/styx` for reference. Use the `/styx` skill for interactive help.

## What Is Styx?

Styx is like JSON but removes everything getting in the way. No required quotes
for bare words, no colons, no commas (optional). Values are untyped until
deserialization — this solves the Norway problem (`country no` is not a boolean).

## Syntax

### Key-value pairs

```styx
name "John Doe"
age 97
host localhost:9000
path /api/v2
```

No quotes needed for bare words (anything without spaces or special chars).

### Sequences (parentheses, space-separated)

```styx
methods (GET POST PUT)
ports (8080 8443 9000)
```

### Objects (braces)

```styx
server {
  host 0.0.0.0
  port 8080
}
```

Inline: `{key value, another thing}`

### Tags (enum variants)

```styx
@none@                              // unit variant
@some(42)                           // tuple variant
@path_prefix{prefix /api}          // struct variant
```

**No space between tag and payload.** `@tag(x)` is one atom; `@tag (x)` is two.

### Unit value

```styx
@                   // unit (like null)
optional_field      // omitted value defaults to @
```

### Comments

```styx
// line comment
/// doc comment (preserved)
```

### Strings

```styx
bare_word           // no quotes needed
"quoted string"     // for spaces
r#"raw "string""#  // no escaping
content <<EOF       // heredoc
multi-line
text here
EOF
```

### Dotted keys

```styx
server.host 0.0.0.0
server.port 8080
// equivalent to: server { host 0.0.0.0, port 8080 }
```

## Rust Deserialization

### With facet-styx

```rust
#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
    tags: Vec<String>,
}

let config: Config = facet_styx::from_str(styx_text)?;
```

### With figue (config from CLI + env + file)

```rust
let config = Figue::<Config>::new()
    .with_styx_file(".config/myapp/config.styx")
    .with_env_prefix("MYAPP")
    .build()?;
```

## Real-World Example

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

## Attributes (key=value shorthand)

For inline object-like data in value position:

```styx
server host>localhost port>8080 tls>true

// Equivalent to:
server {host localhost, port 8080, tls true}
```

## Heredoc Language Hints

```styx
script <<BASH,bash
echo "hello"
BASH

sql <<SQL,sql
SELECT * FROM users
SQL
```

The `,language` suffix enables editor syntax highlighting inside the heredoc.

## Gotchas

### No commas in sequences

```styx
// ERROR
items (a, b, c)

// CORRECT
items (a b c)
```

### Space between tag and payload

```styx
@tag()      // ONE value: tag with empty sequence payload
@tag ()     // TWO values: @tag (unit) and () (empty sequence)
```

### Bare scalars need space before blocks

```styx
config{}    // ERROR
config {}   // CORRECT
```

### Dotted path closure

```styx
a.b {}
a.c {}      // closes a.b
a.b.x 1     // ERROR: a.b was closed
```

## Schema Declarations

```styx
@schema {id crate:myapp-config@1, cli myapp}

// Rest of config follows
name myapp
port 8080
```

## Key Design Decisions

- **Scalars are untyped** — `97`, `true`, `no` are all just text until deserialization
- **Sequences use `()`** — not `[]`. Always space-separated, never comma-separated.
- **Objects use `{}`** — entries separated by newlines or commas
- **Top-level is an implicit object** — no wrapping braces needed
- **Tags for sum types** — `@variant{...}` maps to Rust enums
- **Unit is `@`** — not `null`, not `nil`
- **Doc comments `///`** — preserved in AST, attached to following entry
