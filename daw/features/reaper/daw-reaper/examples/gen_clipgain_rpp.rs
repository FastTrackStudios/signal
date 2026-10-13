use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
fn main() {
    let out = std::env::args().nth(1).unwrap();
    // distinct POSITIONS (0,2,4,6s) and distinct gains, one clip per track
    let p=ReaperProjectBuilder::new().tempo_with_time_sig(120.0,4,4)
    .track("T0",|t|t.item(0.0,1.0,|it|it.name("c0").source_wave("/Users/codywright/Downloads/PNG WORSHIP COLLECTIVE SESSION FILES/10 REASON WHY/Audio Files/10 REASON WHY demo (Bass)_1.1.wav").gain(0.100000)))
    .track("T1",|t|t.item(2.0,1.0,|it|it.name("c1").source_wave("/Users/codywright/Downloads/PNG WORSHIP COLLECTIVE SESSION FILES/10 REASON WHY/Audio Files/10 REASON WHY demo (Drums)_1.1.wav").gain(0.400000)))
    .track("T2",|t|t.item(4.0,1.0,|it|it.name("c2").source_wave("/Users/codywright/Downloads/PNG WORSHIP COLLECTIVE SESSION FILES/10 REASON WHY/Audio Files/10 REASON WHY demo (Guitar)_1.1.wav").gain(0.200000)))
    .track("T3",|t|t.item(6.0,1.0,|it|it.name("c3").source_wave("/Users/codywright/Downloads/PNG WORSHIP COLLECTIVE SESSION FILES/10 REASON WHY/Audio Files/10 REASON WHY demo (Other)_1.1.wav").gain(0.800000)))
    .build();
    std::fs::write(&out, p.to_rpp_string()).unwrap();
}
