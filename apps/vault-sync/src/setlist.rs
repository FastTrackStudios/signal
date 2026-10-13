//! Setlist-note parsing/resolution shared by every consumer of a Task
//! `type: setlist` note (the CLI, the native player, the desktop app):
//! pull the `[[wikilink]]` targets out of a note's body, then resolve
//! each target against a known song library.

use crate::library::LibrarySong;

/// Extract `[[Target]]`/`[[Target|alias]]`/`[[Target#section]]` wikilink
/// targets from a setlist note's body, one per line.
pub fn parse_setlist_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    for line in text.lines() {
        let Some(start) = line.find("[[") else {
            continue;
        };
        let Some(len) = line[start + 2..].find("]]") else {
            continue;
        };
        let inner = &line[start + 2..start + 2 + len];
        let target = inner.split('|').next().unwrap_or(inner);
        let target = target.split('#').next().unwrap_or(target);
        links.push(target.trim().to_string());
    }
    links
}

/// Resolve a wikilink target (a bare title or the full `"Title - Artist"`
/// form) against a known song library.
pub fn resolve_song<'a>(known: &'a [LibrarySong], wanted: &str) -> Result<&'a LibrarySong, String> {
    let link_name = |s: &LibrarySong| format!("{} - {}", s.title, s.artist);
    let matches: Vec<&LibrarySong> = known
        .iter()
        .filter(|s| s.title == wanted || link_name(s) == wanted)
        .collect();
    match matches.as_slice() {
        [song] => Ok(song),
        [] => Err(format!("\"{wanted}\" not found in the library")),
        _ => {
            let options: Vec<_> = matches.iter().map(|s| link_name(s)).collect();
            Err(format!(
                "\"{wanted}\" is ambiguous between: {} — use the full \"Title - Artist\" form",
                options.join(", ")
            ))
        }
    }
}
