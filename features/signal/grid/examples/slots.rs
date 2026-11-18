fn main() {
    let t = signal_proto::defaults::guitar::guitar_rig_template();
    let s = signal_grid::conversion::template_to_grid_slots(&t);
    println!("{} slots", s.len());
    for x in s.iter().take(5) {
        println!(
            "{:?} {:?} col{} row{} tmpl={}",
            x.block_type, x.block_preset_name, x.col, x.row, x.is_template
        );
    }
}
