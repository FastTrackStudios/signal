//! Debug tool: slice a theme's button images to /tmp for visual inspection.
//!
//! ```sh
//! cargo run -p daw-theme-reaper --example dump_slices -- <theme-dir> [names…]
//! ```
fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .expect("usage: dump_slices <theme-dir> [names…]");
    let names: Vec<String> = args.collect();
    let rt = daw_theme_reaper::ReaperTheme::load_dir(&dir).expect("load theme");
    let names = if names.is_empty() {
        vec![
            "track_mute_off".into(),
            "track_solo_off".into(),
            "mcp_io".into(),
        ]
    } else {
        names
    };
    for name in &names {
        match rt.images.button3(name) {
            Ok(s) => {
                let path = format!("/tmp/slice_{name}.png");
                s.normal.save(&path).expect("save");
                println!("{name}: {:?} -> {path}", s.normal.dimensions());
            }
            Err(e) => println!("{name}: {e}"),
        }
    }
}
