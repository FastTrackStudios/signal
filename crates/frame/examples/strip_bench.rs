//! How long a strip takes to record and to draw live, through a
//! [`Recording`] (the cost of building the commands a host's scene holds).
//!
//! ```sh
//! cargo run --release -p theme-runtime --example strip_bench
//! ```

use std::time::Instant;

use frame::theme::runtime::{Recording, Rgba, Schema, Theme, Value};
use kurbo::Affine;

const STRIP: &str = r##"<Document>
  <Variables modes="dark light">
    <Var name="strip.ground" dark="#202020" light="#e8e8e8" />
    <Var name="button" value="#3a3a3a" />
    <Var name="mute.on" value="#d04040" />
    <Var name="meter" value="#40d070" />
    <Var name="cap" value="mix($button, #ffffff, 0.5)" />
  </Variables>
  <Page name="mcp.strip">
    <Frame name="Strip" w=86 h=300 bind:fill="mix($strip.ground, track.color, 0.25)">
      <Frame name="Buttons" x=4 y=4 w=40 h=80 layout="column" gap=2>
        <Frame name="@mute" w=40 h=18 bind:fill="track.mute ? $mute.on : $button" />
        <Frame name="@solo" w=40 h=18 fill="#3a3a3a" bind:visible="h >= 250" />
        <Frame name="@arm" w=40 h=18 fill="#3a3a3a" />
      </Frame>
      <Frame name="Travel" x=50 y=20 w=20 h=240 resize-y="stretch">
        <Frame name="@fader" w=20 h=12 bind:fill="$cap" bind:y="(1 - track.fader) * (ph - 12)" />
      </Frame>
      <Frame name="Well" x=74 y=20 w=8 h=240 resize-y="stretch" fill="#101010">
        <Rectangle name="Level" w=8 h=240 bind:fill="$meter" bind:h="ph * meter.level" bind:y="ph * (1 - meter.level)" />
      </Frame>
      <Text name="@name" x=2 y=280 w=82 h=16 resize-y="bottom" resize-x="stretch" size=11 bind:text="track.name" bind:fill="ink($strip.ground)" bind:visible="w >= 40">Track</Text>
    </Frame>
  </Page>
</Document>
"##;

fn main() {
    let schema = Schema::new()
        .field("track.name")
        .field("track.color")
        .field("track.mute")
        .live("track.fader")
        .live("meter.level");
    let theme = Theme::from_markup(STRIP, schema, None).expect("compiles");
    let fields = |i: usize| {
        vec![
            Value::text(&format!("Track {i}")),
            Value::Color(Rgba::from_hex("#3366cc").unwrap()),
            Value::Bool(i.is_multiple_of(3)),
            Value::Num((i % 10) as f64 / 10.0),
            Value::Num((i % 7) as f64 / 7.0),
        ]
    };
    let strips = 200;
    let size = (86.0, 400.0);

    let t = Instant::now();
    theme.prepare("mcp.strip", size, &fields(0));
    println!(
        "layout, once per size:   {:>8.1} µs",
        t.elapsed().as_secs_f64() * 1e6
    );

    for pass in ["record", "live"] {
        let mut best = f64::MAX;
        let mut cmds = 0;
        for _ in 0..20 {
            let mut rec = Recording::new();
            let t = Instant::now();
            for i in 0..strips {
                let at = Affine::translate((i as f64 * 87.0, 0.0));
                if pass == "record" {
                    theme.record("mcp.strip", size, &fields(i), &mut rec, at);
                } else {
                    theme.live("mcp.strip", size, &fields(i), &mut rec, at);
                }
            }
            best = best.min(t.elapsed().as_secs_f64());
            cmds = rec.len();
        }
        println!(
            "{pass:>6}, {strips} strips:      {:>8.1} µs  ({:.2} µs/strip, {cmds} commands)",
            best * 1e6,
            best * 1e6 / strips as f64
        );
    }
}
