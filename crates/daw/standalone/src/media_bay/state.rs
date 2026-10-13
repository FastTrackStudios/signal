//! Persistent bay state — lives next to `ProjectState`. The live
//! source/item/fx lists rebuild from project data on read; only the
//! bay-specific bits (retained paths, folder layout) need to survive
//! between sessions.

use std::collections::{HashMap, HashSet};

use super::types::{BayFolder, BayView};

/// Persistent bay payload. `Option`-wrapped in `ProjectState` to
/// avoid allocating for projects that never touch the bay.
#[derive(Clone, Debug, Default)]
pub struct BayState {
    /// Paths kept in SourceMedia even when zero items reference them.
    pub retained: HashSet<String>,
    /// Bay folders keyed by `(view, name)`.
    pub folders: HashMap<(BayView, String), BayFolder>,
}

impl BayState {
    /// Serialize to a compact binary form. Format is intentionally
    /// stable — `(version: u32, retained: Vec<String>, folders: Vec<(view, name, entries)>)`.
    pub fn serialize(&self) -> Vec<u8> {
        const VERSION: u32 = 1;
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(&VERSION.to_le_bytes());

        // retained paths
        out.extend_from_slice(&(self.retained.len() as u32).to_le_bytes());
        let mut retained_sorted: Vec<&String> = self.retained.iter().collect();
        retained_sorted.sort();
        for p in retained_sorted {
            write_string(&mut out, p);
        }

        // folders
        out.extend_from_slice(&(self.folders.len() as u32).to_le_bytes());
        let mut folders_sorted: Vec<(&(BayView, String), &BayFolder)> =
            self.folders.iter().collect();
        folders_sorted.sort_by(|a, b| a.0.cmp(b.0));
        for ((view, name), folder) in folders_sorted {
            out.push(view_to_u8(*view));
            write_string(&mut out, name);
            out.extend_from_slice(&(folder.entries.len() as u32).to_le_bytes());
            for e in &folder.entries {
                write_string(&mut out, e);
            }
        }
        out
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader::new(bytes);
        let version = r.read_u32()?;
        if version != 1 {
            return Err(format!("unsupported bay snapshot version {version}"));
        }
        let mut s = Self::default();

        let n_retained = r.read_u32()? as usize;
        for _ in 0..n_retained {
            let p = r.read_string()?;
            s.retained.insert(p);
        }
        let n_folders = r.read_u32()? as usize;
        for _ in 0..n_folders {
            let view = view_from_u8(r.read_u8()?)?;
            let name = r.read_string()?;
            let n_entries = r.read_u32()? as usize;
            let mut entries = Vec::with_capacity(n_entries);
            for _ in 0..n_entries {
                entries.push(r.read_string()?);
            }
            s.folders.insert(
                (view, name.clone()),
                BayFolder {
                    name,
                    view,
                    entries,
                },
            );
        }
        Ok(s)
    }

    /// Merge another `BayState` into this one — additive only
    /// (retained paths union, folder entries union, no removals).
    /// Used by `load_bay`.
    pub fn merge_from(&mut self, other: BayState) {
        self.retained.extend(other.retained);
        for (key, folder) in other.folders {
            let entry = self.folders.entry(key.clone()).or_insert(BayFolder {
                name: folder.name.clone(),
                view: folder.view,
                entries: Vec::new(),
            });
            for e in folder.entries {
                if !entry.entries.contains(&e) {
                    entry.entries.push(e);
                }
            }
        }
    }
}

/// Helper so callers can write `p.bay_state.get_or_create()` without
/// dancing around `Option`. Implemented for `Option<BayState>`.
pub(crate) trait BayStateExt {
    fn get_or_create(&mut self) -> &mut BayState;
    fn as_mut_inner(&mut self) -> Option<&mut BayState>;
}

impl BayStateExt for Option<BayState> {
    fn get_or_create(&mut self) -> &mut BayState {
        if self.is_none() {
            *self = Some(BayState::default());
        }
        self.as_mut().unwrap()
    }
    fn as_mut_inner(&mut self) -> Option<&mut BayState> {
        self.as_mut()
    }
}

// ── Tiny binary writer/reader (no serde dep) ──────────────────

fn write_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn view_to_u8(v: BayView) -> u8 {
    match v {
        BayView::SourceMedia => 0,
        BayView::MediaItems => 1,
        BayView::Effects => 2,
    }
}

fn view_from_u8(b: u8) -> Result<BayView, String> {
    match b {
        0 => Ok(BayView::SourceMedia),
        1 => Ok(BayView::MediaItems),
        2 => Ok(BayView::Effects),
        _ => Err(format!("unknown BayView discriminant {b}")),
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn read_u8(&mut self) -> Result<u8, String> {
        let b = *self
            .buf
            .get(self.pos)
            .ok_or_else(|| "unexpected EOF".to_string())?;
        self.pos += 1;
        Ok(b)
    }
    fn read_u32(&mut self) -> Result<u32, String> {
        let end = self.pos + 4;
        if end > self.buf.len() {
            return Err("unexpected EOF".into());
        }
        let v = u32::from_le_bytes(self.buf[self.pos..end].try_into().unwrap());
        self.pos = end;
        Ok(v)
    }
    fn read_string(&mut self) -> Result<String, String> {
        let len = self.read_u32()? as usize;
        let end = self.pos + len;
        if end > self.buf.len() {
            return Err("unexpected EOF in string".into());
        }
        let s = std::str::from_utf8(&self.buf[self.pos..end])
            .map_err(|e| format!("invalid UTF-8: {e}"))?
            .to_string();
        self.pos = end;
        Ok(s)
    }
}
