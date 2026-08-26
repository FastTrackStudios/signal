//! Generate the watch app's Swift `Codable` mirror of the `/watch/v1` wire
//! DTOs from their facet shapes — Rust stays the source of truth.
//!
//! ```bash
//! cargo run -p signal-guitar-proto --example gen_watch_swift \
//!     > apps/fasttrackstudio/watchos/FTSWatch/Generated/WatchState.generated.swift
//! ```
//!
//! The walker covers exactly what the watch DTOs use — structs of
//! primitives, `String`, `Vec<T>`, `Option<T>` — and fails loudly on
//! anything else so a proto change can't silently drift from Swift.

use std::collections::BTreeMap;

use facet::{Def, Facet, Shape, Type, UserType};
use signal_guitar_proto::watch::WatchState;

fn main() {
    let mut structs = BTreeMap::new();
    collect(WatchState::SHAPE, &mut structs);

    let mut out = String::new();
    out.push_str(
        "// GENERATED — do not edit. Mirrors the facet shapes in\n\
         // features/rigs/guitar/proto/src/watch.rs (the `/watch/v1` wire DTOs).\n\
         // Regenerate: cargo run -p signal-guitar-proto --example gen_watch_swift\n\
         //   > apps/fasttrackstudio/watchos/FTSWatch/Generated/WatchState.generated.swift\n\n\
         import Foundation\n\n",
    );
    for (name, body) in &structs {
        out.push_str(&format!(
            "public struct {name}: Codable, Equatable, Sendable {{\n"
        ));
        for (field, ty) in body {
            out.push_str(&format!("    public var {}: {ty}\n", camel(field)));
        }
        // Memberwise init (public structs don't get one across module
        // boundaries for free).
        out.push_str("\n    public init(\n");
        let params: Vec<String> = body
            .iter()
            .map(|(field, ty)| format!("        {}: {ty}", camel(field)))
            .collect();
        out.push_str(&params.join(",\n"));
        out.push_str("\n    ) {\n");
        for (field, _) in body {
            let f = camel(field);
            out.push_str(&format!("        self.{f} = {f}\n"));
        }
        out.push_str("    }\n\n");
        // The wire is snake_case (facet-json uses the Rust field names).
        out.push_str("    enum CodingKeys: String, CodingKey {\n");
        for (field, _) in body {
            out.push_str(&format!("        case {} = \"{field}\"\n", camel(field)));
        }
        out.push_str("    }\n}\n\n");
    }
    print!("{out}");
}

/// Field list in declaration order: (rust_name, swift_type).
type StructBody = Vec<(String, String)>;

/// Recursively collect every user struct reachable from `shape`.
fn collect(shape: &'static Shape, out: &mut BTreeMap<String, StructBody>) {
    let Type::User(UserType::Struct(st)) = &shape.ty else {
        panic!(
            "gen_watch_swift: expected a struct shape, got {}",
            shape.type_identifier
        );
    };
    if out.contains_key(shape.type_identifier) {
        return;
    }
    let mut body = StructBody::new();
    for field in st.fields {
        body.push((field.name.to_string(), swift_type(field.shape(), out)));
    }
    out.insert(shape.type_identifier.to_string(), body);
}

/// Map a facet shape to its Swift spelling, recursing into user structs.
fn swift_type(shape: &'static Shape, out: &mut BTreeMap<String, StructBody>) -> String {
    match &shape.def {
        Def::List(l) => return format!("[{}]", swift_type(l.t, out)),
        Def::Option(o) => return format!("{}?", swift_type(o.t, out)),
        _ => {}
    }
    match shape.type_identifier {
        "String" => "String".into(),
        "bool" => "Bool".into(),
        "f32" => "Float".into(),
        "f64" => "Double".into(),
        "u8" => "UInt8".into(),
        "u16" => "UInt16".into(),
        "u32" => "UInt32".into(),
        "u64" => "UInt64".into(),
        "i8" => "Int8".into(),
        "i16" => "Int16".into(),
        "i32" => "Int32".into(),
        "i64" => "Int64".into(),
        other => {
            if let Type::User(UserType::Struct(_)) = &shape.ty {
                collect(shape, out);
                other.into()
            } else {
                panic!("gen_watch_swift: unsupported type {other} — extend the walker");
            }
        }
    }
}

/// snake_case → camelCase (Swift field convention).
fn camel(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut upper_next = false;
    for c in s.chars() {
        if c == '_' {
            upper_next = true;
        } else if upper_next {
            result.extend(c.to_uppercase());
            upper_next = false;
        } else {
            result.push(c);
        }
    }
    result
}
