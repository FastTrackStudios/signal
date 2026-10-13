use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
fn main() {
    let out = std::env::args().nth(1).unwrap();
    let p = ReaperProjectBuilder::new()
        .tempo_with_time_sig(120.0, 4, 4)
        .track("PanTrack", |t| {
            t.volume(1.0).pan(0.0).envelope("PANENV2", |e| {
                e.visible()
                    .active()
                    .linear(1.0, -0.75) // 1s: left
                    .linear(3.0, 0.5) // 3s: right
                    .linear(5.0, 0.0)
            })
        }) // 5s: center
        .build();
    std::fs::write(&out, p.to_rpp_string()).unwrap();
}
