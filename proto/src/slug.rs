//! Slugs — the readable, stable names entities are known by across docs.
//!
//! A patch's slug is its identity in the live docs (`patch:<slug>`): what a
//! setlist in Task, an agent and every device call it. It is made once,
//! from the name the patch was first given, and does not follow later
//! renames — renaming "Lead" to "Big Lead" keeps `worship-lead`, so nothing
//! that points at it breaks.

/// A slug from a name, the way the library makes one: an apostrophe drops
/// out (`Mayer's Tone` → `mayers-tone`), any other run of non-alphanumerics
/// is one dash, lower-case, no dash at either end.
///
/// The same rule as a song's library id in Session, so the two read alike.
#[must_use]
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars().filter(|c| *c != '\'' && *c != '\u{2019}') {
        if c.is_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.extend(c.to_lowercase());
        } else {
            dash = true;
        }
    }
    out
}

/// `base`, or `base-2`, `base-3`, … — the first one `taken` does not hold.
///
/// For giving a new patch a slug no other patch in the library has.
#[must_use]
pub fn unique_slug(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = if base.is_empty() { "patch" } else { base };
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}
