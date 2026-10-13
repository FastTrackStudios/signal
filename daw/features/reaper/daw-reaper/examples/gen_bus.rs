//! Generate bus-routing probe RPPs for PTX RE.
//! Usage: cargo run -p daw-reaper --example gen_bus -- <probe> <out.rpp>
use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
fn main() {
    let probe = std::env::args().nth(1).expect("probe");
    let out = std::env::args().nth(2).expect("out");
    let mut b = ReaperProjectBuilder::new().sample_rate(48000);
    match probe.as_str() {
        // 3 plain tracks (baseline for the bus probes)
        "three_plain" => {
            b = b
                .track("SrcA", |t| t)
                .track("SrcB", |t| t)
                .track("BusC", |t| t);
        }
        // SrcA + SrcB both send to BusC (BusC receives from track 0 and 1).
        // Disambiguates: which field carries the SOURCE track identity.
        "bus_two_senders" => {
            b = b
                .track("SrcA", |t| t)
                .track("SrcB", |t| t)
                .track("BusC", |t| t.receive(0).receive(1));
        }
        // Single send, distinct names, for clean isolation vs three_plain.
        "send_one" => {
            b = b
                .track("SrcA", |t| t)
                .track("SrcB", |t| t)
                .track("BusC", |t| t.receive(0));
        }
        _ => panic!("unknown probe {probe}"),
    }
    std::fs::write(&out, b.build().to_rpp_string()).unwrap();
    eprintln!("wrote {out}");
}
