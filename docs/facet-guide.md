# Facet: Rust Reflection & Serialization

[facet-rs/facet](https://github.com/facet-rs/facet) — zero-copy reflection
framework that replaces serde for RPC, config, and serialization.
Clone to `/tmp/facet` for reference.

## What Is Facet?

Facet provides **compile-time shape information** for Rust types via the `Facet`
trait. Unlike serde, shapes are available at compile time for code generation
(TypeScript clients, schema validation) and at runtime for reflection (Peek/Poke).

Used by: roam (RPC serialization), styx (config parsing), tracey (API types).

## Core Derive

```rust
use facet::Facet;

#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
    tags: Vec<String>,
}
```

This generates a `Shape` descriptor that serialization crates can use.

## Derive Attributes

### `#[facet(transparent)]` — Newtype wrapper

```rust
#[derive(Facet)]
#[facet(transparent)]
pub struct SessionId(pub String);
```

Serializes as the inner type (just a `String` on the wire). Essential for
newtype IDs in roam services.

### `#[facet(rename = "...")]` — Field/variant renaming

```rust
#[derive(Facet)]
struct Config {
    #[facet(rename = "server_name")]
    name: String,
}
```

### `#[repr(C)]` or `#[repr(u8)]` — Enum representation

```rust
#[derive(Facet)]
#[repr(u8)]
pub enum Status {
    Active = 0,
    Inactive = 1,
}
```

Required for enums that cross the RPC boundary.

### `#[facet(default)]` — Default values

```rust
#[derive(Facet)]
struct Config {
    #[facet(default)]
    port: u16,  // defaults to 0
}
```

### `#[facet(skip)]` — Skip field

```rust
#[derive(Facet)]
struct State {
    public_data: String,
    #[facet(skip)]
    internal_cache: HashMap<String, String>,
}
```

## Serialization Crates

| Crate | Format | Use Case |
|-------|--------|----------|
| `facet-json` | JSON | API responses, debugging |
| `facet-styx` | Styx | Config files |
| `facet-yaml` | YAML | Legacy config compat |
| `facet-toml` | TOML | Cargo-style config |
| `facet-msgpack` | MessagePack | Binary wire format |
| `facet-postcard` | Postcard | Compact binary (embedded) |

### Serialization API

```rust
// JSON
let json = facet_json::to_string(&config)?;
let config: Config = facet_json::from_str(&json)?;

// Styx
let config: Config = facet_styx::from_str(styx_text)?;

// Postcard (binary)
let bytes = facet_postcard::to_vec(&config)?;
let config: Config = facet_postcard::from_bytes(&bytes)?;
```

## Facet vs Serde

| Feature | Facet | Serde |
|---------|-------|-------|
| Compile-time shapes | Yes (TypeScript codegen) | No |
| Runtime reflection | Yes (Peek/Poke) | No |
| Zero-copy deserialization | Yes | Partial |
| Code generation | TS, Swift (via roam) | No |
| Schema generation | Yes (from shapes) | Via separate crate |
| Ecosystem size | Growing | Massive |
| Performance | Comparable | Battle-tested |

### When to use Facet vs Serde

- **Facet**: All types that cross RPC boundaries (roam services), config files
  (styx/figue), API types shared with TypeScript
- **Serde**: File formats with existing serde support (e.g., `dawfile-reaper`
  for RPP parsing), third-party crate interop

### Dual-derive (when you need both)

```rust
use facet::Facet;
use serde::{Serialize, Deserialize};

#[derive(Facet, Serialize, Deserialize)]
struct SharedType {
    name: String,
    value: u32,
}
```

## Roam Integration

Roam requires all service parameter and return types to implement `Facet`.
The `#[roam::service]` macro generates dispatchers/clients that use facet
for serialization.

```rust
#[derive(Facet)]
pub struct CreateSessionRequest {
    pub project: String,
    pub branch: Option<String>,
}

#[derive(Facet)]
pub enum CreateSessionResponse {
    Created { session: SessionInfo },
    Failed { message: String },
}

#[roam::service]
pub trait MyService {
    async fn create_session(&self, req: CreateSessionRequest) -> CreateSessionResponse;
}
```

## Complete Attribute Reference

### Container attributes

| Attribute | Purpose |
|-----------|---------|
| `#[facet(transparent)]` | Newtype — serialize as inner type |
| `#[facet(opaque)]` | Fields don't need to impl Facet |
| `#[facet(untagged)]` | Enum: no discriminator tag |
| `#[facet(is_numeric)]` | Enum: serialize by discriminant value |
| `#[facet(tag = "type")]` | Internally tagged enum |
| `#[facet(tag = "t", content = "c")]` | Adjacently tagged enum |
| `#[facet(rename_all = "camelCase")]` | Case conversion (camelCase, snake_case, PascalCase, kebab-case, SCREAMING_SNAKE_CASE) |
| `#[facet(deny_unknown_fields)]` | Reject unknown fields on deser |
| `#[facet(pod)]` | Plain Old Data — safe mutation via Poke |
| `#[facet(cow)]` | Cow-like enum (Borrowed/Owned variants) |
| `#[facet(metadata_container)]` | One value field + N metadata fields |

### Field attributes

| Attribute | Purpose |
|-----------|---------|
| `#[facet(rename = "...")]` | Rename field on wire |
| `#[facet(alias = "...")]` | Accept alternate name on deser |
| `#[facet(default)]` | Use `Default::default()` if missing |
| `#[facet(default = expr)]` | Custom default (e.g., `default = 8080`) |
| `#[facet(skip)]` | Skip both ser and deser |
| `#[facet(skip_serializing)]` | Skip on ser only |
| `#[facet(skip_deserializing)]` | Skip on deser only |
| `#[facet(skip_serializing_if = fn)]` | Conditional skip (e.g., `Option::is_none`) |
| `#[facet(flatten)]` | Flatten nested struct into parent |
| `#[facet(sensitive)]` | Redact in debug/pretty output |
| `#[facet(child)]` | Mark as child node (XML-like) |
| `#[facet(recursive_type)]` | For self-referential types (`Vec<Self>`) |
| `#[facet(proxy = Type)]` | Custom ser via `TryFrom` intermediary |
| `#[facet(metadata = "span")]` | Metadata field (excluded from hashing) |

### Enum variant attributes

| Attribute | Purpose |
|-----------|---------|
| `#[facet(other)]` | Catch-all for unknown variant names |
| `#[facet(rename = "...")]` | Rename variant on wire |

## Enum Tagging Strategies

```rust
// Externally tagged (default)
#[derive(Facet)]
enum Message { Text(String), Data { bytes: Vec<u8> } }
// {"Text": "hello"} or {"Data": {"bytes": [...]}}

// Internally tagged
#[derive(Facet)]
#[facet(tag = "type")]
enum Event { Click { x: i32, y: i32 }, Scroll { delta: i32 } }
// {"type": "Click", "x": 10, "y": 20}

// Adjacently tagged
#[derive(Facet)]
#[facet(tag = "t", content = "c")]
enum Value { Int(i32), Str(String) }
// {"t": "Int", "c": 42}

// Untagged
#[derive(Facet)]
#[facet(untagged)]
enum Any { Int(i32), Float(f64), Str(String) }
// 42 or 3.14 or "hello"
```

## Reflection (Peek/Poke)

```rust
use facet_reflect::{Peek, Partial};

// Read-only inspection
let peek = my_value.peek();

// Build values incrementally
let mut partial = Partial::new(Config::SHAPE);
partial.set_field("name", "app".into());
partial.set_field("port", 8080);
let config: Config = partial.finish()?;
```

## Extension Attributes

Format crates define custom attributes without proc-macros:

```rust
// facet-xml defines:
#[derive(Facet)]
struct Element {
    #[facet(xml::attribute)]
    id: String,
    #[facet(xml::text)]
    content: String,
}
```

Typos caught at compile time with suggestions.

## Best Practices

1. **`#[facet(transparent)]` for all newtype IDs** — `ProfileId`, `RigId`, etc.
2. **`#[repr(C)]` on enums** that cross RPC — ensures stable wire representation
3. **Derive both `Facet` and `Clone`** on types used in services
4. **Use `Facet` for all new types** — only use serde when interfacing with
   existing serde-only crates
5. **Group params into structs** — max 4-tuple for Facet (roam constraint)
6. **`#[facet(sensitive)]`** on API keys, tokens, passwords
7. **`#[facet(skip_serializing_if = Option::is_none)]`** for optional fields
8. **`#[facet(flatten)]`** to compose structs without nesting on the wire
9. **`#[facet(default = value)]`** for config fields with sensible defaults
10. **`#[facet(recursive_type)]`** for tree structures (`Vec<Self>`, `Box<Self>`)
