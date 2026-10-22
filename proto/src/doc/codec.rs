//! The value codec: a serde JSON tree ⇄ Loro containers, written as a
//! reconcile.
//!
//! The mapping (doc schema 1):
//!
//! | JSON                                       | Loro                                   |
//! | ------------------------------------------ | -------------------------------------- |
//! | object                                     | `Map`, one entry per field             |
//! | array of objects, each with a unique string `id` | keyed list: `Map { "$list": "keyed", order: MovableList<id>, items: Map<id, Map> }` |
//! | any other array                            | one value (`List`), last writer wins   |
//! | `null` / bool / string                     | `Null` / `Bool` / `String`             |
//! | integer (fits `i64`) / float               | `I64` / `Double` (bit-exact)           |
//!
//! Every field is its own map entry, so two people changing different
//! fields of one entity both land. Lists of things with ids — a rig's
//! scenes, a song's scenes, a setlist's entries — are keyed lists: an entry
//! is edited in place, a reorder is a move, and an agent adding a song while
//! someone renames another both land. A list without ids (a patch's
//! overrides, a scene's engine selections) is one value: two people
//! changing it at once is last-writer-wins for the whole list.
//!
//! Writing compares with what the doc holds and emits operations only for
//! what differs, so writing the same value twice records nothing.

use std::collections::HashSet;

use loro::{Container, LoroMap, LoroMovableList, LoroResult, LoroValue, ValueOrContainer};
use serde_json::{Map, Number, Value};

/// Marks a map that holds a keyed list.
pub const LIST_MARK: &str = "$list";
const KEYED: &str = "keyed";
const ORDER: &str = "order";
const ITEMS: &str = "items";

enum Shape<'a> {
    Object(&'a Map<String, Value>),
    Keyed(Vec<(&'a str, &'a Map<String, Value>)>),
    Value,
}

/// How a JSON value is laid out in the doc.
fn shape(value: &Value) -> Shape<'_> {
    match value {
        Value::Object(map) => Shape::Object(map),
        Value::Array(items) if !items.is_empty() => {
            let mut seen = HashSet::with_capacity(items.len());
            let mut keyed = Vec::with_capacity(items.len());
            for item in items {
                let Value::Object(map) = item else {
                    return Shape::Value;
                };
                let Some(Value::String(id)) = map.get("id") else {
                    return Shape::Value;
                };
                if !seen.insert(id.as_str()) {
                    return Shape::Value;
                }
                keyed.push((id.as_str(), map));
            }
            Shape::Keyed(keyed)
        }
        _ => Shape::Value,
    }
}

/// Reconcile `map[key]` with `value`.
///
/// # Errors
/// When a Loro operation fails.
pub fn put(map: &LoroMap, key: &str, value: &Value) -> LoroResult<()> {
    match shape(value) {
        Shape::Object(object) => sync_object(&child_map(map, key, false)?, object),
        Shape::Keyed(items) => sync_keyed(&child_map(map, key, true)?, &items),
        Shape::Value => put_value(map, key, to_loro(value)),
    }
}

/// Set a plain value, unless the map already holds exactly it.
///
/// # Errors
/// When a Loro operation fails.
pub fn put_value(map: &LoroMap, key: &str, value: LoroValue) -> LoroResult<()> {
    if let Some(ValueOrContainer::Value(current)) = map.get(key)
        && current == value
    {
        return Ok(());
    }
    map.insert(key, value)
}

/// The map container at `map[key]` of the right kind, made if missing (or
/// if a value or the other kind of map is there).
fn child_map(map: &LoroMap, key: &str, keyed: bool) -> LoroResult<LoroMap> {
    if let Some(ValueOrContainer::Container(Container::Map(child))) = map.get(key)
        && is_keyed(&child) == keyed
    {
        return Ok(child);
    }
    let child = map.insert_container(key, LoroMap::new())?;
    if keyed {
        child.insert(LIST_MARK, KEYED)?;
    }
    Ok(child)
}

fn is_keyed(map: &LoroMap) -> bool {
    matches!(
        map.get(LIST_MARK),
        Some(ValueOrContainer::Value(LoroValue::String(s))) if s.as_str() == KEYED
    )
}

fn sync_object(map: &LoroMap, object: &Map<String, Value>) -> LoroResult<()> {
    for (key, value) in object {
        put(map, key, value)?;
    }
    let stale: Vec<String> = map
        .keys()
        .map(|k| k.to_string())
        .filter(|k| !object.contains_key(k.as_str()))
        .collect();
    for key in stale {
        map.delete(&key)?;
    }
    Ok(())
}

fn sync_keyed(list: &LoroMap, items: &[(&str, &Map<String, Value>)]) -> LoroResult<()> {
    let store = match list.get(ITEMS) {
        Some(ValueOrContainer::Container(Container::Map(store))) => store,
        _ => list.insert_container(ITEMS, LoroMap::new())?,
    };
    let order = match list.get(ORDER) {
        Some(ValueOrContainer::Container(Container::MovableList(order))) => order,
        _ => list.insert_container(ORDER, LoroMovableList::new())?,
    };

    for (id, item) in items {
        sync_object(&child_map(&store, id, false)?, item)?;
    }
    let wanted: HashSet<&str> = items.iter().map(|(id, _)| *id).collect();
    let gone: Vec<String> = store
        .keys()
        .map(|k| k.to_string())
        .filter(|k| !wanted.contains(k.as_str()))
        .collect();
    for id in gone {
        store.delete(&id)?;
    }

    let target: Vec<&str> = items.iter().map(|(id, _)| *id).collect();
    sync_order(&order, &target)
}

/// Make `order` read `target` with the fewest operations: drop what is gone
/// (and any duplicate a concurrent insert left), move what is out of place,
/// insert what is new.
fn sync_order(order: &LoroMovableList, target: &[&str]) -> LoroResult<()> {
    let wanted: HashSet<&str> = target.iter().copied().collect();
    let mut current: Vec<String> = order
        .to_vec()
        .into_iter()
        .map(|v| match v {
            LoroValue::String(s) => s.to_string(),
            _ => String::new(),
        })
        .collect();

    let mut seen = HashSet::new();
    let mut drop = Vec::new();
    for (pos, id) in current.iter().enumerate() {
        if !wanted.contains(id.as_str()) || !seen.insert(id.clone()) {
            drop.push(pos);
        }
    }
    for pos in drop.into_iter().rev() {
        order.delete(pos, 1)?;
        current.remove(pos);
    }

    for (pos, id) in target.iter().enumerate() {
        if current.get(pos).map(String::as_str) == Some(*id) {
            continue;
        }
        if let Some(from) = current.iter().position(|c| c == id) {
            order.mov(from, pos)?;
            let moved = current.remove(from);
            current.insert(pos, moved);
        } else {
            order.insert(pos, *id)?;
            current.insert(pos, (*id).to_owned());
        }
    }
    Ok(())
}

// ── reading ─────────────────────────────────────────────────────────────

/// What `map[key]` holds, as JSON (`None` when the key is absent).
#[must_use]
pub fn get(map: &LoroMap, key: &str) -> Option<Value> {
    match map.get(key)? {
        ValueOrContainer::Value(value) => Some(from_deep(&value)),
        ValueOrContainer::Container(Container::Map(child)) => {
            Some(from_deep(&child.get_deep_value()))
        }
        ValueOrContainer::Container(Container::MovableList(child)) => {
            Some(from_deep(&child.get_deep_value()))
        }
        ValueOrContainer::Container(Container::List(child)) => {
            Some(from_deep(&child.get_deep_value()))
        }
        ValueOrContainer::Container(_) => Some(Value::Null),
    }
}

/// A deep Loro value as JSON, keyed lists read back as arrays.
#[must_use]
pub fn from_deep(value: &LoroValue) -> Value {
    match value {
        LoroValue::Map(map) if is_keyed_value(map) => read_keyed(map),
        LoroValue::Map(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), from_deep(v))).collect())
        }
        LoroValue::List(items) => Value::Array(items.iter().map(from_deep).collect()),
        LoroValue::Null => Value::Null,
        LoroValue::Bool(b) => Value::Bool(*b),
        // Non-finite doubles have no JSON form; serde_json writes them as null too.
        LoroValue::Double(d) => Number::from_f64(*d).map_or(Value::Null, Value::Number),
        LoroValue::I64(i) => Value::Number((*i).into()),
        LoroValue::String(s) => Value::String(s.to_string()),
        LoroValue::Binary(bytes) => {
            Value::Array(bytes.iter().map(|b| Value::Number((*b).into())).collect())
        }
        LoroValue::Container(id) => Value::String(id.to_string()),
    }
}

fn is_keyed_value(map: &loro::LoroMapValue) -> bool {
    matches!(map.get(LIST_MARK), Some(LoroValue::String(s)) if s.as_str() == KEYED)
}

/// A keyed list in order. An id in `order` with no item (deleted while
/// someone moved it) is skipped, a repeat (two peers inserted it at once)
/// counted once, and an item missing from `order` goes at the end, by id.
fn read_keyed(map: &loro::LoroMapValue) -> Value {
    let empty = loro::LoroMapValue::default();
    let items = match map.get(ITEMS) {
        Some(LoroValue::Map(items)) => items,
        _ => &empty,
    };
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(items.len());
    if let Some(LoroValue::List(order)) = map.get(ORDER) {
        for id in order.iter() {
            if let LoroValue::String(id) = id
                && let Some(item) = items.get(id.as_str())
                && seen.insert(id.to_string())
            {
                out.push(from_deep(item));
            }
        }
    }
    let mut rest: Vec<(&String, &LoroValue)> = items
        .iter()
        .filter(|(id, _)| !seen.contains(id.as_str()))
        .collect();
    rest.sort_by(|a, b| a.0.cmp(b.0));
    out.extend(rest.into_iter().map(|(_, item)| from_deep(item)));
    Value::Array(out)
}

/// JSON → a plain Loro value (no containers).
#[must_use]
pub fn to_loro(value: &Value) -> LoroValue {
    match value {
        Value::Null => LoroValue::Null,
        Value::Bool(b) => LoroValue::Bool(*b),
        Value::Number(n) => n.as_i64().map_or_else(
            // A float (or a u64 beyond i64): `as_f64` is exact for floats.
            || LoroValue::Double(n.as_f64().unwrap_or(f64::NAN)),
            LoroValue::I64,
        ),
        Value::String(s) => LoroValue::String(s.as_str().into()),
        Value::Array(items) => LoroValue::List(items.iter().map(to_loro).collect()),
        Value::Object(map) => {
            LoroValue::Map(map.iter().map(|(k, v)| (k.clone(), to_loro(v))).collect())
        }
    }
}
