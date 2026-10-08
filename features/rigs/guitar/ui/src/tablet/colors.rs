//! Colours for songs and sections, and a set's name read into its parts —
//! the touch prototype's `setlist/colors.ts` and `setlist/sets.ts`.
//!
//! A song's colour is its name, encoded: the same name always lands on the
//! same hue (FNV-1a over the lowercased name, as UTF-16 like the prototype,
//! into a fixed palette), unless the player gives it one. A section's colour
//! is its type: Intro sky, Verse emerald, Chorus blue, Bridge violet, …; a
//! Pre- or Post- section takes its parent's light shade.

/// The hues a song can be: none of them the live green or the danger red.
pub const SONG_PALETTE: [&str; 14] = [
    "#38bdf8", "#60a5fa", "#818cf8", "#a78bfa", "#c084fc", "#e879f9", "#f472b6", "#fb7185", "#fb923c",
    "#fbbf24", "#facc15", "#a3e635", "#2dd4bf", "#22d3ee",
];

/// FNV-1a, 32-bit, over UTF-16 code units (JavaScript's `charCodeAt`).
#[must_use]
pub fn fnv1a(text: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for unit in text.encode_utf16() {
        h ^= u32::from(unit);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// A colour from a name alone.
#[must_use]
pub fn name_colour(name: &str) -> &'static str {
    SONG_PALETTE[fnv1a(&name.trim().to_lowercase()) as usize % SONG_PALETTE.len()]
}

/// A song's colour: the player's, else its name's.
#[must_use]
pub fn song_colour(name: &str, chosen: &str) -> String {
    if chosen.trim().is_empty() { name_colour(name).to_string() } else { chosen.trim().to_string() }
}

const SLATE: &str = "#94a3b8";

fn section_base(n: &str) -> Option<&'static str> {
    let starts = |p: &[&str]| p.iter().any(|x| n.starts_with(x));
    Some(if starts(&["intro"]) {
        "#38bdf8"
    } else if starts(&["verse", "instrumental"]) {
        "#34d399"
    } else if starts(&["chorus", "refrain"]) {
        "#3b82f6"
    } else if starts(&["bridge"]) {
        "#a78bfa"
    } else if starts(&["outro", "ending"]) {
        "#fbbf24"
    } else if starts(&["interlude"]) {
        "#facc15"
    } else if starts(&["solo"]) {
        "#fb7185"
    } else if starts(&["vamp", "turnaround"]) {
        "#a3e635"
    } else if starts(&["tag"]) {
        "#d8b4fe"
    } else {
        return None;
    })
}

fn light(c: &str) -> &'static str {
    match c {
        "#38bdf8" => "#bae6fd",
        "#34d399" => "#a7f3d0",
        "#3b82f6" => "#bfdbfe",
        "#a78bfa" => "#ddd6fe",
        "#fbbf24" => "#fde68a",
        "#facc15" => "#fef08a",
        "#fb7185" => "#fecdd3",
        "#a3e635" => "#d9f99d",
        "#d8b4fe" => "#f3e8ff",
        _ => SLATE,
    }
}

/// A section's colour from its type, read off its name.
#[must_use]
pub fn section_colour(name: &str) -> &'static str {
    let n = name.trim().to_lowercase();
    for pre in ["pre", "post"] {
        if let Some(rest) = n.strip_prefix(pre) {
            let rest = rest.trim_start_matches([' ', '-']);
            return section_base(rest).map_or(SLATE, light);
        }
    }
    section_base(&n).unwrap_or(SLATE)
}

// ── Sets ───────────────────────────────────────────────────────────────────

/// A set as an event on a date, with a title only when the night has one.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SetMeta {
    pub event: String,
    /// `YYYY-MM-DD`, or empty.
    pub date: String,
    pub title: String,
}

/// "HSM 10-6-26 Worship Night" → HSM · 2026-10-06 · Worship Night. A name
/// without a date is all event.
#[must_use]
pub fn parse_set_name(name: &str) -> SetMeta {
    let words: Vec<&str> = name.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        let nums: Option<Vec<u32>> = w.split('-').map(|p| p.parse().ok()).collect();
        if let Some(nums) = nums
            && let [m, d, y] = nums[..]
            && (1..=12).contains(&m)
            && (1..=31).contains(&d)
            && i > 0
        {
            let year = if y < 100 { 2000 + y } else { y };
            return SetMeta {
                event: words[..i].join(" "),
                date: format!("{year}-{m:02}-{d:02}"),
                title: words[i + 1..].join(" "),
            };
        }
    }
    SetMeta { event: name.trim().to_string(), ..SetMeta::default() }
}

/// A set's parts: the stored ones where it has them, else read off its name.
#[must_use]
pub fn set_meta(name: &str, event: &str, date: &str, title: &str) -> SetMeta {
    let parsed = parse_set_name(name);
    SetMeta {
        event: if event.is_empty() { parsed.event } else { event.to_string() },
        date: if date.is_empty() { parsed.date } else { date.to_string() },
        title: if title.is_empty() { parsed.title } else { title.to_string() },
    }
}

/// What the set is called on screen: its title, else its event.
#[must_use]
pub fn set_heading(m: &SetMeta) -> String {
    let t = m.title.trim();
    if !t.is_empty() {
        return t.to_string();
    }
    let e = m.event.trim();
    if e.is_empty() { "Untitled set".to_string() } else { e.to_string() }
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Day of the week, 0 = Sunday (Sakamoto).
fn weekday(y: i32, m: u32, d: u32) -> usize {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    ((y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + d as i32).rem_euclid(7)) as usize
}

/// `YYYY-MM-DD` → "Tue 6 Oct"; empty when it is not a date.
#[must_use]
pub fn date_label(iso: &str) -> String {
    let p: Vec<&str> = iso.split('-').collect();
    let (Some(y), Some(m), Some(d)) = (
        p.first().and_then(|x| x.parse::<i32>().ok()),
        p.get(1).and_then(|x| x.parse::<u32>().ok()),
        p.get(2).and_then(|x| x.parse::<u32>().ok()),
    ) else {
        return String::new();
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return String::new();
    }
    format!("{} {d} {}", WEEKDAYS[weekday(y, m, d)], MONTHS[(m - 1) as usize])
}

fn ymd(iso: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(iso.trim(), "%Y-%m-%d").ok()
}

/// Today, `YYYY-MM-DD`.
#[must_use]
pub fn today_iso() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// `iso` moved by `days` (today when it is not a date).
#[must_use]
pub fn add_days(iso: &str, days: i64) -> String {
    let d = ymd(iso).unwrap_or_else(|| chrono::Local::now().date_naive());
    (d + chrono::Duration::days(days)).format("%Y-%m-%d").to_string()
}

/// A date's month and weekday, short, and its day: ("Oct", 6, "Tue").
#[must_use]
pub fn date_parts(iso: &str) -> Option<(&'static str, u32, &'static str)> {
    use chrono::Datelike;
    let d = ymd(iso)?;
    Some((MONTHS[d.month0() as usize], d.day(), WEEKDAYS[d.weekday().num_days_from_sunday() as usize]))
}

/// "Today", "Tomorrow", "Yesterday", "In 5 days", "3 weeks ago".
#[must_use]
pub fn when_label(iso: &str) -> String {
    let Some(d) = ymd(iso) else { return String::new() };
    let days = (d - chrono::Local::now().date_naive()).num_days();
    match days {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        2..=6 => format!("In {days} days"),
        7..=13 => "Next week".into(),
        14.. => format!("In {} weeks", (days as f64 / 7.0).round()),
        -6..=-2 => format!("{} days ago", -days),
        -13..=-7 => "Last week".into(),
        _ => format!("{} weeks ago", (-days as f64 / 7.0).round()),
    }
}

/// The stored name, in the house style: event, M-D-YY, title.
#[must_use]
pub fn set_name(m: &SetMeta) -> String {
    use chrono::Datelike;
    let mut parts = vec![m.event.trim().to_string()];
    if let Some(d) = ymd(&m.date) {
        parts.push(format!("{}-{}-{:02}", d.month(), d.day(), d.year() % 100));
    }
    if !m.title.trim().is_empty() {
        parts.push(m.title.trim().to_string());
    }
    parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

/// The next date an event usually falls on: a week after its latest set,
/// today when it has none (or the latest is long past).
#[must_use]
pub fn next_date_for(event: &str, sets: &[SetMeta]) -> String {
    let today = today_iso();
    let mut dates: Vec<&str> = sets.iter().filter(|s| s.event.eq_ignore_ascii_case(event) && !s.date.is_empty()).map(|s| s.date.as_str()).collect();
    dates.sort_unstable();
    let Some(last) = dates.last() else { return today };
    let mut next = add_days(last, 7);
    while next < today {
        next = add_days(&next, 7);
    }
    next
}

/// The next section a song's form suggests (Intro → Verse 1 → Chorus 1 …).
#[must_use]
pub fn suggest_section(names: &[String]) -> String {
    if names.is_empty() {
        return "Intro".into();
    }
    let count = |base: &str| names.iter().filter(|n| n.to_lowercase().starts_with(&base.to_lowercase())).count();
    let (verses, choruses) = (count("Verse"), count("Chorus"));
    if verses <= choruses { format!("Verse {}", verses + 1) } else { format!("Chorus {}", choruses + 1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_colours_match_the_prototype() {
        // FNV-1a("washed") — the same hue the prototype paints.
        assert_eq!(fnv1a(""), 0x811c_9dc5);
        assert_eq!(fnv1a("a"), 0xe40c_292c);
        assert_eq!(name_colour("WASHED"), name_colour("washed"));
    }

    #[test]
    fn sections_by_type() {
        assert_eq!(section_colour("Chorus 2"), "#3b82f6");
        assert_eq!(section_colour("Pre-Chorus"), "#bfdbfe");
        assert_eq!(section_colour("Dance! (V2)"), SLATE);
    }

    #[test]
    fn set_names_are_built_in_the_house_style() {
        let m = SetMeta { event: "HSM".into(), date: "2026-10-06".into(), title: "Worship Night".into() };
        assert_eq!(set_name(&m), "HSM 10-6-26 Worship Night");
        assert_eq!(parse_set_name(&set_name(&m)), m);
        assert_eq!(add_days("2026-10-06", 7), "2026-10-13");
        assert_eq!(date_parts("2026-10-06"), Some(("Oct", 6, "Tue")));
        assert_eq!(suggest_section(&["Intro".into(), "Verse 1".into()]), "Chorus 1");
    }

    #[test]
    fn set_names_read_into_parts() {
        let m = parse_set_name("HSM 10-6-26 Worship Night");
        assert_eq!(m, SetMeta { event: "HSM".into(), date: "2026-10-06".into(), title: "Worship Night".into() });
        assert_eq!(date_label("2026-10-06"), "Tue 6 Oct");
        assert_eq!(set_heading(&parse_set_name("CYA 7-9-26")), "CYA");
    }
}
