//! Print the SVG a control renders to.
//!
//! For when a PNG comes out blank or wrong in a way the pixels cannot
//! explain. It found a stray `<g>` that had swallowed the closing
//! `</defs>`, which put an entire plate inside the definitions block —
//! valid SVG, renders nothing, and looks from the outside exactly like a
//! component that has stopped drawing.
//!
//!     cargo run -p daw-theme-art --example markup -- transport_play_on

fn main() {
    for name in std::env::args().skip(1) {
        match daw_theme_art::cell_markup(&name, Default::default()) {
            Some(svg) => println!("--- {name}\n{svg}\n"),
            None => println!("--- {name}: no vector control draws it"),
        }
    }
}
