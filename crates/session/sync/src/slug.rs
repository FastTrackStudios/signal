//! A song's library id — the name every machine knows a song by in the
//! library, and in a set Task keeps.

/// A song's library id from its title, the way the library makes one: an
/// apostrophe drops out (`God, I'm` → `god-im`), any other run of
/// non-alphanumerics is one dash, lower-case, no dash at either end.
#[must_use]
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.chars().filter(|c| *c != '\'' && *c != '\u{2019}') {
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

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn a_title_becomes_the_librarys_id() {
        assert_eq!(slugify("God, I'm Just Grateful"), "god-im-just-grateful");
        assert_eq!(slugify("Always On Time"), "always-on-time");
        assert_eq!(slugify("  Who Else? "), "who-else");
    }
}
