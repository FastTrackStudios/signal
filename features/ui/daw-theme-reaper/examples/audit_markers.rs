//! Audit every theme image's marker decode: flag degenerate margins
//! (dropped → uniform stretch) and stretch-zone hits.
fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| {
        "/home/cody/Development/FastTrackStudio/reaper-theme/extracted/antitheme".into()
    });
    let theme = daw_theme_reaper::ReaperTheme::load_dir(&dir).expect("theme");
    let mut degenerate = Vec::new();
    let mut marked = 0;
    let names: Vec<String> = theme.images.names().map(String::from).collect();
    for name in names {
        let Ok(s) = theme.images.load(&name) else {
            continue;
        };
        let m = &s.markers;
        let (w, h) = (s.image.width(), s.image.height());
        let any = m.fixed_left + m.fixed_right + m.fixed_top + m.fixed_bottom > 0;
        if any {
            marked += 1;
        }
        let degen_x = m.fixed_left + m.fixed_right >= w && m.fixed_left + m.fixed_right > 0;
        let degen_y = m.fixed_top + m.fixed_bottom >= h && m.fixed_top + m.fixed_bottom > 0;
        if degen_x || degen_y {
            degenerate.push(format!(
                "{name}: {w}x{h} l={} r={} t={} b={}{}{}",
                m.fixed_left,
                m.fixed_right,
                m.fixed_top,
                m.fixed_bottom,
                if degen_x { " DEGEN-X" } else { "" },
                if degen_y { " DEGEN-Y" } else { "" },
            ));
        }
    }
    println!("{marked} marked images; {} degenerate:", degenerate.len());
    for d in &degenerate {
        println!("  {d}");
    }
}
