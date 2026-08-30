//! The preset library and browsing model shared by every FTS plugin UI.
//!
//! EQ, Reverb and Compressor all need the same thing: a list of presets, a way
//! to search and filter it, a selection that arrow keys and next/previous
//! buttons can move, and a parameter set to hand the DSP when one is chosen.
//! None of that is specific to what the plugin does, so none of it lives in
//! the plugin.
//!
//! # Headless on purpose
//!
//! No Dioxus, no rendering — the same model drives the plugin editors, a TUI,
//! the CLI and tests. `preset-browser-ui` renders it. (This mirrors how
//! `signal-browser` splits from `signal-ui`; that one browses the *collection*
//! — rigs, engines, packs — while this browses one processor's presets.)
//!
//! # A preset is parameters, not a patch format
//!
//! [`Preset::parameters`] is a plain list of `(name, value)` pairs, which is
//! exactly what `NativeEq::set_named` / `NativeReverb::set_named` take. That
//! keeps one library type usable for every processor, and it is already the
//! shape the translated Valhalla and FabFilter presets are stored in.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub mod library;

pub use library::{load_directory, LoadError, LoadReport};

/// One preset: a named parameter set, plus what a browser needs to find it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Preset {
    /// Display name, e.g. "Snare Plate".
    pub name: String,
    /// Grouping shown in the browser — a folder, bank or family.
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// Free-form tags, matched by search alongside the name.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Where this came from, for provenance in the UI ("from Valhalla
    /// VintageVerb / Plates").
    #[serde(default)]
    pub origin: Option<String>,
    /// The parameter set to apply, by name.
    #[serde(default)]
    pub parameters: Vec<(String, f64)>,
    /// How closely this preset was measured to match the reference it was
    /// translated from — the worst per-octave decay ratio error, where 0 is
    /// exact. `None` for presets that were never measured.
    ///
    /// Worth carrying into the UI: a translated library is not uniformly
    /// faithful, and a browser that can say "this one is exact and that one
    /// is a near miss" is more honest than one that presents them alike.
    #[serde(default)]
    pub match_error: Option<f64>,
}

impl Preset {
    /// The text a search query is matched against.
    fn haystack(&self) -> String {
        let mut s = self.name.to_lowercase();
        for extra in [self.category.as_deref(), self.author.as_deref(), self.origin.as_deref()]
            .into_iter()
            .flatten()
        {
            s.push(' ');
            s.push_str(&extra.to_lowercase());
        }
        for t in &self.tags {
            s.push(' ');
            s.push_str(&t.to_lowercase());
        }
        s
    }
}

/// How the visible list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortMode {
    /// Alphabetical by name.
    #[default]
    Name,
    /// Grouped by category, alphabetical within it.
    Category,
    /// The order the library was loaded in — for a bank whose own order means
    /// something.
    Library,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            SortMode::Name => "Name",
            SortMode::Category => "Category",
            SortMode::Library => "Library",
        }
    }

    /// The next mode, for a control that cycles rather than opening a menu.
    pub fn cycle(self) -> Self {
        match self {
            SortMode::Name => SortMode::Category,
            SortMode::Category => SortMode::Library,
            SortMode::Library => SortMode::Name,
        }
    }
}

/// A loaded set of presets, plus the browsing state over it.
///
/// The state is deliberately part of the same type: a plugin editor opens,
/// filters, scrolls and selects, and keeping that beside the data means the
/// UI holds one thing and the tests drive the same thing.
#[derive(Debug, Clone, Default)]
pub struct PresetBrowser {
    presets: Vec<Preset>,
    query: String,
    category: Option<String>,
    sort: SortMode,
    /// Index into `presets`, not into the visible list — a selection survives
    /// the filter changing under it.
    selected: Option<usize>,
}

impl PresetBrowser {
    pub fn new(presets: Vec<Preset>) -> Self {
        Self {
            presets,
            ..Default::default()
        }
    }

    /// Every preset, unfiltered.
    pub fn all(&self) -> &[Preset] {
        &self.presets
    }

    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }

    /// Every category present, sorted, for a filter control.
    pub fn categories(&self) -> Vec<String> {
        self.presets
            .iter()
            .filter_map(|p| p.category.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    // ── Filtering ──────────────────────────────────────────────────────────

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Set the search text. Matching is case-insensitive and spans the name,
    /// category, author, origin and tags, so "bManic" or "plate" both find
    /// things without the user knowing which field they are in.
    ///
    /// Multiple words must all match, in any order — typing "dark plate"
    /// should find "Plate — Dark Vocal" without the user guessing the word
    /// order.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    pub fn category_filter(&self) -> Option<&str> {
        self.category.as_deref()
    }

    pub fn set_category_filter(&mut self, category: Option<String>) {
        self.category = category;
    }

    pub fn sort_mode(&self) -> SortMode {
        self.sort
    }

    pub fn set_sort_mode(&mut self, sort: SortMode) {
        self.sort = sort;
    }

    /// Clear the search and the category filter, leaving the selection alone.
    pub fn clear_filters(&mut self) {
        self.query.clear();
        self.category = None;
    }

    fn matches(&self, preset: &Preset) -> bool {
        if let Some(want) = &self.category {
            if preset.category.as_deref() != Some(want.as_str()) {
                return false;
            }
        }
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        let haystack = preset.haystack();
        query.split_whitespace().all(|word| haystack.contains(word))
    }

    /// Indices of the presets that pass the filters, in sort order.
    ///
    /// Indices rather than references so a caller can pair them with
    /// [`Self::select`] without borrowing the browser for the whole render.
    pub fn visible(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..self.presets.len())
            .filter(|&i| self.matches(&self.presets[i]))
            .collect();
        match self.sort {
            SortMode::Library => {}
            SortMode::Name => out.sort_by_key(|&i| self.presets[i].name.to_lowercase()),
            SortMode::Category => out.sort_by_key(|&i| {
                (
                    self.presets[i]
                        .category
                        .clone()
                        .unwrap_or_default()
                        .to_lowercase(),
                    self.presets[i].name.to_lowercase(),
                )
            }),
        }
        out
    }

    pub fn visible_count(&self) -> usize {
        self.presets.iter().filter(|p| self.matches(p)).count()
    }

    // ── Selection ──────────────────────────────────────────────────────────

    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected(&self) -> Option<&Preset> {
        self.selected.and_then(|i| self.presets.get(i))
    }

    /// Select by index into the full library. Out-of-range clears it.
    pub fn select(&mut self, index: usize) -> Option<&Preset> {
        self.selected = (index < self.presets.len()).then_some(index);
        self.selected()
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Step through the *visible* list — what a next/previous button does.
    ///
    /// Stepping from nothing selected starts at the first visible preset, and
    /// stepping from a selection the filter has hidden also starts fresh
    /// rather than jumping somewhere arbitrary. Does not wrap: reaching the
    /// end of a bank should feel like the end of it.
    pub fn step(&mut self, delta: isize) -> Option<&Preset> {
        let visible = self.visible();
        if visible.is_empty() {
            return None;
        }
        let next = match self.selected.and_then(|s| visible.iter().position(|&i| i == s)) {
            Some(pos) => {
                let target = pos as isize + delta;
                target.clamp(0, visible.len() as isize - 1) as usize
            }
            None if delta < 0 => visible.len() - 1,
            None => 0,
        };
        self.selected = Some(visible[next]);
        self.selected()
    }

    pub fn select_next(&mut self) -> Option<&Preset> {
        self.step(1)
    }

    pub fn select_previous(&mut self) -> Option<&Preset> {
        self.step(-1)
    }

    /// The parameters of the current selection, ready for `set_named`.
    pub fn selected_parameters(&self) -> &[(String, f64)] {
        self.selected().map(|p| p.parameters.as_slice()).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: &str, category: &str, tags: &[&str]) -> Preset {
        Preset {
            name: name.into(),
            category: Some(category.into()),
            author: None,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            origin: None,
            parameters: vec![("decay_time".into(), 2.0)],
            match_error: None,
        }
    }

    fn browser() -> PresetBrowser {
        PresetBrowser::new(vec![
            preset("Snare Plate", "Plates", &["drums", "bright"]),
            preset("Dark Vocal Plate", "Plates", &["vocals"]),
            preset("Acoustic Chamber", "Chambers", &["room"]),
            preset("Big Hall", "Halls", &["large"]),
        ])
    }

    #[test]
    fn everything_is_visible_before_any_filtering() {
        let b = browser();
        assert_eq!(b.visible().len(), 4);
        assert_eq!(b.visible_count(), 4);
        assert!(!b.is_empty());
    }

    #[test]
    fn categories_are_deduplicated_and_sorted() {
        assert_eq!(browser().categories(), ["Chambers", "Halls", "Plates"]);
    }

    #[test]
    fn search_is_case_insensitive_and_spans_fields() {
        let mut b = browser();
        b.set_query("PLATE");
        assert_eq!(b.visible_count(), 2, "matches the name");

        b.set_query("drums");
        assert_eq!(b.visible_count(), 1, "matches a tag");

        b.set_query("chambers");
        assert_eq!(b.visible_count(), 1, "matches the category");
    }

    #[test]
    fn search_words_match_in_any_order() {
        // Typing what you remember, not what the author typed.
        let mut b = browser();
        b.set_query("dark plate");
        assert_eq!(b.visible_count(), 1);
        b.set_query("plate dark");
        assert_eq!(b.visible_count(), 1);
    }

    #[test]
    fn a_query_matching_nothing_yields_nothing_rather_than_everything() {
        let mut b = browser();
        b.set_query("zzz");
        assert_eq!(b.visible_count(), 0);
        assert!(b.visible().is_empty());
    }

    #[test]
    fn whitespace_only_query_is_treated_as_no_query() {
        let mut b = browser();
        b.set_query("   ");
        assert_eq!(b.visible_count(), 4);
    }

    #[test]
    fn category_filter_and_search_combine() {
        let mut b = browser();
        b.set_category_filter(Some("Plates".into()));
        assert_eq!(b.visible_count(), 2);
        b.set_query("vocal");
        assert_eq!(b.visible_count(), 1);

        b.clear_filters();
        assert_eq!(b.visible_count(), 4);
    }

    #[test]
    fn sort_modes_order_differently_and_cycle() {
        let mut b = browser();
        b.set_sort_mode(SortMode::Name);
        let names: Vec<&str> = b.visible().iter().map(|&i| b.all()[i].name.as_str()).collect();
        assert_eq!(names[0], "Acoustic Chamber");

        b.set_sort_mode(SortMode::Category);
        let cats: Vec<&str> = b
            .visible()
            .iter()
            .filter_map(|&i| b.all()[i].category.as_deref())
            .collect();
        assert_eq!(cats[0], "Chambers");

        b.set_sort_mode(SortMode::Library);
        assert_eq!(b.visible(), vec![0, 1, 2, 3]);

        assert_eq!(SortMode::Name.cycle(), SortMode::Category);
        assert_eq!(SortMode::Library.cycle(), SortMode::Name);
    }

    #[test]
    fn stepping_walks_the_visible_list_and_stops_at_the_ends() {
        let mut b = browser();
        b.set_sort_mode(SortMode::Library);

        assert_eq!(b.select_next().map(|p| p.name.clone()), Some("Snare Plate".into()));
        assert_eq!(b.select_next().map(|p| p.name.clone()), Some("Dark Vocal Plate".into()));
        assert_eq!(b.select_previous().map(|p| p.name.clone()), Some("Snare Plate".into()));

        // Does not wrap at either end.
        assert_eq!(b.select_previous().map(|p| p.name.clone()), Some("Snare Plate".into()));
        for _ in 0..10 {
            b.select_next();
        }
        assert_eq!(b.selected().map(|p| p.name.clone()), Some("Big Hall".into()));
    }

    #[test]
    fn stepping_backwards_from_nothing_starts_at_the_end() {
        let mut b = browser();
        b.set_sort_mode(SortMode::Library);
        assert_eq!(b.select_previous().map(|p| p.name.clone()), Some("Big Hall".into()));
    }

    #[test]
    fn stepping_skips_presets_the_filter_hides() {
        let mut b = browser();
        b.set_sort_mode(SortMode::Library);
        b.set_category_filter(Some("Plates".into()));
        b.select_next();
        b.select_next();
        // Only two plates: stepping again stays on the second, never reaching
        // the chamber that follows it in the library.
        b.select_next();
        assert_eq!(b.selected().map(|p| p.name.clone()), Some("Dark Vocal Plate".into()));
    }

    #[test]
    fn a_selection_survives_the_filter_changing_under_it() {
        // The selection indexes the library, not the visible list, so
        // narrowing the filter does not silently re-point it at something else.
        let mut b = browser();
        b.select(3); // Big Hall
        b.set_category_filter(Some("Plates".into()));
        assert_eq!(b.selected().map(|p| p.name.clone()), Some("Big Hall".into()));

        // ...and stepping from a hidden selection starts fresh rather than
        // jumping somewhere arbitrary.
        b.set_sort_mode(SortMode::Library);
        assert_eq!(b.select_next().map(|p| p.name.clone()), Some("Snare Plate".into()));
    }

    #[test]
    fn selecting_out_of_range_clears_rather_than_panicking() {
        let mut b = browser();
        b.select(2);
        assert!(b.selected().is_some());
        assert!(b.select(99).is_none());
        assert!(b.selected().is_none());
    }

    #[test]
    fn an_empty_library_is_safe_to_drive() {
        let mut b = PresetBrowser::default();
        assert!(b.is_empty());
        assert!(b.visible().is_empty());
        assert!(b.select_next().is_none());
        assert!(b.select_previous().is_none());
        assert!(b.selected_parameters().is_empty());
        assert!(b.categories().is_empty());
    }

    #[test]
    fn selected_parameters_are_what_the_dsp_is_given() {
        let mut b = browser();
        b.select(0);
        assert_eq!(b.selected_parameters(), &[("decay_time".to_string(), 2.0)]);
        b.clear_selection();
        assert!(b.selected_parameters().is_empty());
    }
}
