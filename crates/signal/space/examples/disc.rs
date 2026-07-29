fn main() {
    let t = std::time::Instant::now();
    let d = signal_space::discover_spaces();
    println!("discover: {:?} in {:?}", d, t.elapsed());
    let t = std::time::Instant::now();
    let f = signal_space::find_space("luke-pieces");
    println!("find: {} in {:?}", f.is_some(), t.elapsed());
}
