//! Print the measured sprite-cell bounds for a few images.
//!
//! A vector control draws into the cell it is *told* about, so when the
//! drawing lands half a pixel off, the first thing to check is whether the
//! cell it was handed matches the one the compositor measured.

fn main() {
    let dir = std::path::Path::new("features/reaper/fts-theme/FastTrackStudio/.source-art");
    for name in std::env::args().skip(1) {
        let path = dir.join(format!("{name}.png"));
        let img = match image::open(&path) {
            Ok(i) => i.to_rgba8(),
            Err(e) => {
                eprintln!("{name}: {e}");
                continue;
            }
        };
        let cells = daw_theme_art::derive::cell_bounds(&img);
        println!(
            "{name}: {}x{}  cells {:?}",
            img.width(),
            img.height(),
            cells
        );
    }
}
