//! **Finding a song in the set** — the search field at the top of the
//! setlist sidebar.
//!
//! Incremental and case-insensitive: every word typed must turn up in the
//! song's title or its key (`"wash"`, `"amaz b"`, `"c#"`). Punctuation is
//! forgiven, so `"dont"` finds "Don't Stop". The filter only hides rows; the
//! songs keep their set positions, so the song up stays the song up and
//! clearing the field gives back the whole list as it was.

use dioxus::prelude::*;

use super::tokens::*;

/// Lowercased, with only its letters, digits and spaces left.
fn bare(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether a song (its title, its key) answers `query`. An empty query (or
/// only spaces) matches every song.
#[must_use]
pub fn song_matches(query: &str, name: &str, key: &str) -> bool {
    let hay = format!("{name} {key}").to_lowercase();
    let hay_bare = bare(&hay);
    query.split_whitespace().all(|word| {
        let word = word.to_lowercase();
        if hay.contains(&word) {
            return true;
        }
        let word = bare(&word);
        !word.is_empty() && hay_bare.contains(&word)
    })
}

/// The positions (in set order) of the songs that answer `query`; songs
/// are `(title, key)`.
#[must_use]
pub fn matching_songs<'a>(
    query: &str,
    songs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<usize> {
    songs
        .into_iter()
        .enumerate()
        .filter(|(_, (name, key))| song_matches(query, name, key))
        .map(|(i, _)| i)
        .collect()
}

/// Whether `query` filters anything at all.
#[must_use]
pub fn is_filtering(query: &str) -> bool {
    !query.trim().is_empty()
}

/// The search field: a magnifier, the text, and a × to clear it (Escape
/// clears it too). Writes `query` as it is typed.
#[component]
pub fn SongSearch(query: Signal<String>) -> Element {
    let mut query = query;
    let q = query();
    rsx! {
        div { style: "flex-shrink: 0; padding: 8px 12px; border-bottom: 1px solid {RULE};",
            label {
                style: "display: flex; align-items: center; gap: 8px; height: 40px; padding: 0 4px 0 12px; border-radius: {R}; background: {FILL};",
                svg { width: "15", height: "15", view_box: "0 0 16 16", style: "flex-shrink: 0;",
                    circle { cx: "7", cy: "7", r: "4.8", fill: "none", stroke: INK_3, stroke_width: "1.6" }
                    path { d: "M10.6 10.6 14 14", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
                }
                input {
                    "data-testid": "setlist-search",
                    "aria-label": "Find a song",
                    value: "{q}",
                    placeholder: "Find a song",
                    style: "flex: 1; min-width: 0; height: 100%; border: none; background: transparent; color: {INK}; font-size: 15px; font-family: {FONT};",
                    oninput: move |e| query.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Escape {
                            query.set(String::new());
                        }
                    },
                }
                if !q.is_empty() {
                    button {
                        "data-testid": "setlist-search-clear",
                        "aria-label": "Clear the search",
                        title: "Clear",
                        style: "width: 36px; height: 36px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border: none; background: transparent; padding: 0; cursor: pointer;",
                        onclick: move |_| query.set(String::new()),
                        svg { width: "12", height: "12", view_box: "0 0 12 12",
                            path { d: "M2 2l8 8M10 2l-8 8", stroke: INK_3, stroke_width: "1.7", stroke_linecap: "round" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SET: [(&str, &str); 5] = [
        ("Amazing!", "B"),
        ("Washed", "B"),
        ("Takeover", "C# sig (A#m7-D#m7-F#)"),
        ("Don't Stop", "G"),
        ("Way Maker", "E"),
    ];

    fn hits(q: &str) -> Vec<usize> {
        matching_songs(q, SET.iter().copied())
    }

    #[test]
    fn an_empty_query_keeps_every_song() {
        assert_eq!(hits(""), vec![0, 1, 2, 3, 4]);
        assert_eq!(hits("   "), vec![0, 1, 2, 3, 4]);
        assert!(!is_filtering("  "));
        assert!(is_filtering("w"));
    }

    #[test]
    fn titles_match_incrementally_and_ignore_case() {
        assert_eq!(hits("w"), vec![1, 4]);
        assert_eq!(hits("WA"), vec![1, 4]);
        assert_eq!(hits("wash"), vec![1]);
        assert_eq!(hits("washed"), vec![1]);
        assert_eq!(hits("washedx"), Vec::<usize>::new());
    }

    #[test]
    fn every_word_must_match_title_or_key() {
        // "b" is Amazing's and Washed's key; "amaz" narrows it to one.
        assert_eq!(hits("amaz b"), vec![0]);
        assert_eq!(hits("b"), vec![0, 1]);
        assert_eq!(hits("way e"), vec![4]);
        assert_eq!(hits("way g"), Vec::<usize>::new());
    }

    #[test]
    fn keys_match_with_their_sharps() {
        assert_eq!(hits("c#"), vec![2]);
        assert_eq!(hits("A#m7"), vec![2]);
    }

    #[test]
    fn punctuation_is_forgiven() {
        assert_eq!(hits("dont"), vec![3]);
        assert_eq!(hits("don't"), vec![3]);
        assert_eq!(hits("amazing"), vec![0]);
        // Punctuation typed on its own still matches literally.
        assert_eq!(hits("!"), vec![0]);
    }
}
