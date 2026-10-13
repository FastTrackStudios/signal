//! Evaluate a WALTER snippet from stdin and dump all attrs (debug tool).
use std::io::Read;
fn main() {
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src).unwrap();
    let mut env = daw_ui::reaper_theme::walter::Env::new();
    env.set("w", 110.0);
    env.set("h", 600.0);
    env.set("Scale", 1.0);
    let out = daw_ui::reaper_theme::walter::evaluate(&src, None, &env);
    let mut keys: Vec<_> = out.attrs.keys().collect();
    keys.sort();
    for k in keys {
        println!("{k} = {:?}", out.attrs[k]);
    }
}
