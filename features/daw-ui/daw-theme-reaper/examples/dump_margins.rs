//! Print the pink margins of button images (debug).
fn main() {
    let dir = std::env::args().nth(1).expect("theme dir");
    let rt = daw_theme_reaper::ReaperTheme::load_dir(&dir).unwrap();
    for name in [
        "mcp_io",
        "mcp_env",
        "track_fx_empty",
        "track_fxon_h",
        "track_mute_off",
        "mcp_recmode_in",
        "track_fx_in_empty",
    ] {
        match rt.images.button3(name) {
            Ok(s) => println!(
                "{name}: state {:?} markers {:?}",
                s.normal.dimensions(),
                s.markers
            ),
            Err(e) => println!("{name}: {e}"),
        }
    }
}
