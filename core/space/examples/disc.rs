fn main() {
    let t = std::time::Instant::now();
    let d = signal_space::engine::discover_spaces();
    println!("discover: {:?} in {:?}", d, t.elapsed());
    let t = std::time::Instant::now();
    let f = signal_space::engine::find_space("luke-pieces");
    println!("find: {} in {:?}", f.is_some(), t.elapsed());
}
