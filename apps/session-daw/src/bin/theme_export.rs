//! Write the painters' mixer strip out as a theme: REAPER's layout to the
//! pixel, the reference the native strip is checked against.
//!
//! ```sh
//! cargo run -r -p session-daw --bin theme_export              # to apps/session-daw/themes/fts-dark.frame
//! cargo run -r -p session-daw --bin theme_export -- out.frame # elsewhere
//! ```
//!
//! The console theme (`apps/session-daw/themes/console/`) is not written
//! here: it is its files, edited as they are. See
//! `session_daw::skin::export`.

fn main() {
    let palette = session_daw::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "apps/session-daw/themes/fts-dark.frame".to_owned());
    let themes = vec![(out, session_daw::skin::export::fts_dark(&palette))];
    for (out, markup) in themes {
        // Compiled before it is written: a theme the mixer would refuse is
        // not worth overwriting a good one with.
        if let Err(errors) =
            theme_runtime::Theme::from_markup(&markup, session_daw::skin::schema(), None)
        {
            eprintln!("{out}:");
            for e in errors.iter().take(20) {
                eprintln!("  {e}");
            }
            std::process::exit(1);
        }
        std::fs::write(&out, &markup).unwrap_or_else(|e| panic!("{out}: {e}"));
        println!("wrote {out} ({} lines)", markup.lines().count());
    }
}
