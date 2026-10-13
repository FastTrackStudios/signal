//! Print how a chart parses: sections, their bars, and the first voicings.
fn main() {
    let path = std::env::args().nth(1).expect("chart path");
    let text = std::fs::read_to_string(path).expect("read");
    let chart = keyflow::text::chart::parse_chart(&text).expect("parse");
    let layout = session::setlist::chart_import::chart_to_layout(&text).expect("layout");
    println!(
        "key {:?}  sections {}  layout sections {}",
        chart.initial_key.as_ref().map(ToString::to_string),
        chart.sections.len(),
        layout.sections.len()
    );
    for (i, s) in chart.sections.iter().enumerate() {
        let bars: usize = s.tracks.iter().map(|t| t.measures.len()).max().unwrap_or(0);
        let l = layout
            .sections
            .get(i)
            .map(|l| (l.start_seconds, l.measures));
        println!(
            "  {i:>2} {:<14} bars {:>2}  layout {:?}",
            format!("{:?}", s.section.section_type),
            bars,
            l
        );
    }
    for v in session::keyflow::generate::voicings(&chart, 4)
        .iter()
        .take(12)
    {
        println!(
            "  m{:>3} b{:.1} {:>4.1}b {:?} {}",
            v.measure, v.beat, v.beats, v.pitches, v.symbol
        );
    }
}
