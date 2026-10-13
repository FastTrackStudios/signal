//! Text params and inline editing: a `TextField` control edits its bound
//! text param in place — caret, selection, typing (IME commits arrive as
//! text too), Backspace / Delete, arrows (Shift extends), Home / End,
//! Cmd/Ctrl-A, Enter or Tab to commit, Escape to cancel.
//!
//! Text is measured by the host's font when it hands one in
//! ([`Runtime::set_text_measure`]); otherwise by an estimate.

use std::sync::Arc;

use document_proto::{NodeId, TextAlign, TextData};
use surface_proto::{ControlKind, ParamAddr, SlotRole, Source, TextValue};

use super::ui::Edit;
use super::{EngineCall, Runtime};
use crate::gesture::{Key, Modifiers};

/// Advance width of `text` set in a text node's style (family, weight,
/// style, width, size, letter spacing).
pub type TextMeasure = Arc<dyn Fn(&TextData, &str) -> f64 + Send + Sync>;

/// The estimate used without a font: Inter's average advance.
#[must_use]
pub fn estimate(text: &str, size: f64, letter_spacing: f64) -> f64 {
    text.chars().map(|c| if c.is_uppercase() { 0.66 } else if c == ' ' { 0.28 } else { 0.55 } * size + letter_spacing).sum()
}

/// What a text field shows while it is edited, in its value node's own
/// coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct EditView {
    pub text: String,
    /// The caret's x.
    pub caret_x: f64,
    /// The selection's x span, when there is one.
    pub selection: Option<(f64, f64)>,
}

fn byte_at(s: &str, char_index: usize) -> usize {
    s.char_indices().nth(char_index).map_or(s.len(), |(b, _)| b)
}

impl Runtime {
    /// Measure text with the host's font from now on.
    pub fn set_text_measure(&mut self, measure: TextMeasure) {
        self.text_measure = Some(measure);
    }

    fn measure(&self, data: &TextData, text: &str) -> f64 {
        match &self.text_measure {
            Some(m) => m(data, text),
            None => estimate(text, data.font_size, data.letter_spacing),
        }
    }

    /// Every text param's value the runtime holds.
    #[must_use]
    pub fn texts(&self) -> Vec<TextValue> {
        self.texts
            .iter()
            .map(|(addr, text)| TextValue {
                addr: addr.clone(),
                text: text.clone(),
            })
            .collect()
    }

    /// Text values from the engine. A field being edited keeps its buffer.
    pub fn apply_texts(&mut self, texts: impl IntoIterator<Item = TextValue>) {
        for t in texts {
            self.texts.insert(t.addr, t.text);
        }
    }

    /// The param a text field edits.
    fn field_addr(&self, id: NodeId) -> Option<(ParamAddr, usize)> {
        let c = self.surface.controls.get(&id)?;
        let ControlKind::TextField { max_len, .. } = c.kind else {
            return None;
        };
        match &c.binding.as_ref()?.source {
            Source::Param { addr } => Some((
                addr.clone(),
                if max_len == 0 {
                    usize::MAX
                } else {
                    max_len as usize
                },
            )),
            _ => None,
        }
    }

    /// Start editing text field `id` (everything selected, or the caret at
    /// the end).
    pub fn begin_edit(&mut self, id: NodeId, select_all: bool) -> bool {
        let Some((addr, max_len)) = self.field_addr(id) else {
            return false;
        };
        let text = self.texts.get(&addr).cloned().unwrap_or_default();
        let len = text.chars().count();
        self.ui.edit = Some(Edit {
            node: id,
            addr,
            buffer: text.clone(),
            caret: len,
            anchor: if select_all { 0 } else { len },
            original: text,
            max_len,
        });
        self.ui.focus = Some(id);
        true
    }

    /// Stop editing, keeping the text (trimmed).
    pub fn commit_edit(&mut self) -> Vec<EngineCall> {
        let mut calls = Vec::new();
        if let Some(e) = self.ui.edit.take() {
            self.set_text(&e.addr, e.buffer.trim().to_string(), &mut calls);
        }
        self.update_active();
        calls
    }

    /// Stop editing, back to the text it had.
    pub fn cancel_edit(&mut self) -> Vec<EngineCall> {
        let mut calls = Vec::new();
        if let Some(e) = self.ui.edit.take() {
            self.set_text(&e.addr, e.original, &mut calls);
        }
        self.update_active();
        calls
    }

    /// Typed text (a key's characters, an IME commit) into the field being
    /// edited. `None` when nothing is edited.
    pub fn type_text(&mut self, text: &str) -> Option<Vec<EngineCall>> {
        let e = self.ui.edit.as_mut()?;
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        if clean.is_empty() {
            return Some(Vec::new());
        }
        let (lo, hi) = (e.caret.min(e.anchor), e.caret.max(e.anchor));
        let room = e
            .max_len
            .saturating_sub(e.buffer.chars().count() - (hi - lo));
        let insert: String = clean.chars().take(room).collect();
        let (bl, bh) = (byte_at(&e.buffer, lo), byte_at(&e.buffer, hi));
        e.buffer.replace_range(bl..bh, &insert);
        e.caret = lo + insert.chars().count();
        e.anchor = e.caret;
        Some(self.edit_changed())
    }

    fn edit_changed(&mut self) -> Vec<EngineCall> {
        let mut calls = Vec::new();
        if let Some(e) = &self.ui.edit {
            let (addr, text) = (e.addr.clone(), e.buffer.clone());
            self.set_text(&addr, text, &mut calls);
        }
        calls
    }

    pub(crate) fn edit_key(&mut self, key: Key, mods: Modifiers) -> Vec<EngineCall> {
        let Some(e) = self.ui.edit.as_mut() else {
            return Vec::new();
        };
        let len = e.buffer.chars().count();
        let (lo, hi) = (e.caret.min(e.anchor), e.caret.max(e.anchor));
        let mut changed = false;
        let to = |e: &mut Edit, at: usize| {
            e.caret = at.min(len);
            if !mods.fine {
                e.anchor = e.caret;
            }
        };
        match key {
            Key::Left if lo != hi && !mods.fine => {
                e.caret = lo;
                e.anchor = lo;
            }
            Key::Right if lo != hi && !mods.fine => {
                e.caret = hi;
                e.anchor = hi;
            }
            Key::Left => to(e, e.caret.saturating_sub(1)),
            Key::Right => to(e, e.caret + 1),
            Key::Home | Key::Up | Key::PageUp => to(e, 0),
            Key::End | Key::Down | Key::PageDown => to(e, len),
            Key::Backspace | Key::Delete => {
                let (from, until) = match key {
                    _ if lo != hi => (lo, hi),
                    Key::Backspace => (lo.saturating_sub(1), lo),
                    _ => (lo, (lo + 1).min(len)),
                };
                if from != until {
                    let (b0, b1) = (byte_at(&e.buffer, from), byte_at(&e.buffer, until));
                    e.buffer.replace_range(b0..b1, "");
                    changed = true;
                }
                e.caret = from;
                e.anchor = from;
            }
            Key::Char(c) if mods.alt && c.eq_ignore_ascii_case(&'a') => {
                e.anchor = 0;
                e.caret = len;
            }
            Key::Char(c) if !mods.alt => {
                let s = c.to_string();
                return self.type_text(&s).unwrap_or_default();
            }
            Key::Enter | Key::Tab => return self.commit_edit(),
            Key::Escape => return self.cancel_edit(),
            _ => {}
        }
        if changed {
            self.edit_changed()
        } else {
            Vec::new()
        }
    }

    /// The value node a field shows its text in, with its style.
    fn field_text(&self, id: NodeId) -> Option<(NodeId, TextData, f64)> {
        let c = self.surface.controls.get(&id)?;
        let node = c.slots_of(SlotRole::Value).next().map(NodeId)?;
        let n = self.surface.scene.node(node)?;
        Some((node, n.text()?.clone(), n.size.width))
    }

    /// Where the text starts in its node (its alignment in its box).
    fn text_left(&self, data: &TextData, width: f64, text: &str) -> f64 {
        let w = self.measure(data, text);
        match data.align {
            TextAlign::Center => ((width - w) / 2.0).max(0.0),
            TextAlign::Right => (width - w).max(0.0),
            _ => 0.0,
        }
    }

    /// Put the caret nearest box-space x (Shift extends the selection).
    pub(crate) fn place_caret(&mut self, id: NodeId, x: f64, extend: bool) {
        let Some((node, data, width)) = self.field_text(id) else {
            return;
        };
        let Some(b) = self.box_of(node) else { return };
        let Some(text) = self.ui.edit.as_ref().map(|e| e.buffer.clone()) else {
            return;
        };
        let left = b.x + self.text_left(&data, width, &text);
        let mut best = (0usize, f64::MAX);
        let chars: Vec<char> = text.chars().collect();
        for i in 0..=chars.len() {
            let prefix: String = chars[..i].iter().collect();
            let d = (left + self.measure(&data, &prefix) - x).abs();
            if d < best.1 {
                best = (i, d);
            }
        }
        if let Some(e) = &mut self.ui.edit {
            e.caret = best.0;
            if !extend {
                e.anchor = best.0;
            }
        }
    }

    /// What field `id` shows while edited (`None` when it isn't).
    #[must_use]
    pub fn edit_view(&self, id: NodeId) -> Option<EditView> {
        let e = self.ui.edit.as_ref().filter(|e| e.node == id)?;
        let (_, data, width) = self.field_text(id)?;
        let left = self.text_left(&data, width, &e.buffer);
        let x_of = |i: usize| {
            let prefix: String = e.buffer.chars().take(i).collect();
            left + self.measure(&data, &prefix)
        };
        let (lo, hi) = (e.caret.min(e.anchor), e.caret.max(e.anchor));
        Some(EditView {
            text: e.buffer.clone(),
            caret_x: x_of(e.caret),
            selection: (lo != hi).then(|| (x_of(lo), x_of(hi))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_grows_with_text() {
        assert!(estimate("Kick", 10.0, 0.0) > estimate("Ki", 10.0, 0.0));
        assert_eq!(estimate("", 10.0, 1.0), 0.0);
        assert_eq!(byte_at("aé b", 2), 3);
    }
}
