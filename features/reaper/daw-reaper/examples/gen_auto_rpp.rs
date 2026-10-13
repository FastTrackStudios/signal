// Author an RPP with volume/pan/mute automation envelopes so the external
// converter (RPP->PTX) produces a PTX we can parse to RE the automation format.
use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
fn main() {
    let out = std::env::args().nth(1).unwrap();
    let p = ReaperProjectBuilder::new()
        .tempo_with_time_sig(120.0, 4, 4)
        // Track WITH automation
        .track("AutoTrack", |t| {
            t.volume(1.0)
                .pan(0.0)
                .envelope("VOLENV2", |e| {
                    e.visible()
                        .active()
                        .linear(0.0, 1.0) // 0dB
                        .linear(2.0, 0.5) // ~-6dB
                        .linear(4.0, 2.0) // +6dB
                        .linear(6.0, 1.0)
                })
                .envelope("PANENV2", |e| {
                    e.visible()
                        .active()
                        .linear(0.0, 0.0)
                        .linear(2.0, -1.0) // hard left
                        .linear(4.0, 1.0) // hard right
                        .linear(6.0, 0.0)
                })
                .envelope("MUTEENV", |e| {
                    e.visible()
                        .active()
                        .square(0.0, 1.0)
                        .square(3.0, 0.0) // mute from 3s
                        .square(5.0, 1.0)
                })
        })
        // Control track WITHOUT automation (for diffing)
        .track("PlainTrack", |t| t.volume(1.0).pan(0.0))
        .build();
    std::fs::write(&out, p.to_rpp_string()).unwrap();
    eprintln!("wrote {}", out);
}
