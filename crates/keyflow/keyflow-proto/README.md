Keyflow Proto API
=================

This crate is the shared data model for Keyflow charts. For most callers, use the
clean facade under `api::` instead of the full surface area.

Quick start
-----------

```rust
use keyflow_proto::api::parse;

let chart = parse::chart(chart_text)?;
```

Prelude
-------

```rust
use keyflow_proto::api::prelude::*;

let key = Key::parse("F#")?;
let section = SectionType::parse("VS")?;
```

Why `api::`?
-----------
- Small, discoverable surface
- String-first helpers for parsing
- No service/RPC wiring required

RPC / services
--------------
Service traits and request/response types live under `services::`. Use those if
you need Roam RPC integration.
