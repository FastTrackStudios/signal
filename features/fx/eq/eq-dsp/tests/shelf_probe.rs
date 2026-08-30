#[test]
fn probe_shelf() {
    use eq_dsp::band::Band;
    use eq_dsp::design::FilterType;
    let sr = 48000.0;
    // Drive a tone through a real Band and measure, so this is the path the
    // plugin actually takes rather than the design helper in isolation.
    let measure = |order: usize, f: f64| -> f64 {
        let mut b = Band::new();
        b.filter_type = FilterType::HighShelf;
        b.freq_hz = 10000.0;
        b.gain_db = 3.0;
        b.q = 1.0;
        b.order = order;
        b.enabled = true;
        b.update(sr);
        let inc = std::f64::consts::TAU * f / sr;
        let n = 48000;
        let mut acc_in = 0.0;
        let mut acc_out = 0.0;
        for i in 0..n {
            let x = (inc * i as f64).sin();
            let y = b.tick(x, 0);
            if i > n / 2 {
                acc_in += x * x;
                acc_out += y * y;
            }
        }
        10.0 * (acc_out / acc_in).log10()
    };
    for order in [1usize, 2, 3, 4] {
        let row: Vec<String> = [100.0, 1000.0, 5000.0, 10000.0, 20000.0]
            .iter()
            .map(|f| format!("{:>8.2}", measure(order, *f)))
            .collect();
        eprintln!("order {order}: {}", row.join(" "));
    }
}
