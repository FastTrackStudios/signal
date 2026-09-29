use super::*;
use signal_guitar_proto::BlockParam;

fn block(id: &str, bt: BlockType, params: &[(&str, f32, f32, f32)], options: &[&str], option: u32) -> LiveBlock {
    LiveBlock {
        id: id.into(),
        block_type: bt,
        name: id.into(),
        bypassed: false,
        param_name: params.first().map(|p| p.0.to_string()),
        param_value: 0.0,
        param_min: 0.0,
        param_max: 1.0,
        params: params.iter().map(|(n, v, lo, hi)| BlockParam { name: (*n).into(), value: *v, min: *lo, max: *hi, overridden: false }).collect(),
        preset: String::new(),
        options: options.iter().map(ToString::to_string).collect(),
        option,
        overridden: false,
    }
}

fn ranges() -> Ranges {
    Arc::new(|_, name: &str| match name {
        "time" => Some(ParameterRange { min: 20.0, max: 2000.0, taper: Taper::Logarithmic, unit: SigUnit::Milliseconds }),
        "mix" => Some(ParameterRange { min: 0.0, max: 1.0, taper: Taper::Linear, unit: SigUnit::Percent }),
        _ => None,
    })
}

fn chain() -> Vec<LiveBlock> {
    vec![
        block("Delay", BlockType::Delay, &[("time", 350.0, 20.0, 2000.0), ("mix", 0.2, 0.0, 1.0)], &[], 0),
        block("Delay 2", BlockType::Delay, &[("mix", 0.5, 0.0, 1.0)], &["Tape", "Digital", "Ping-Pong"], 1),
    ]
}

#[test]
fn catalog_covers_params_toggles_choices_and_actions() {
    let perf = PerformanceModel::default();
    let c = guitar_catalog(&chain(), &perf, &[], &ranges());
    let time = c.param(&ParamAddr::new("Delay/time")).unwrap();
    assert_eq!(time.range.scale, Scale::Logarithmic, "taper from the DSP range");
    assert_eq!(time.range.unit, Unit::Milliseconds);
    assert_eq!(time.group.as_deref(), Some("Delay"));
    assert!(c.param(&ParamAddr::new("Delay 2/on")).is_some());
    let ty = c.param(&ParamAddr::new("Delay 2/type")).unwrap();
    assert_eq!(ty.display(0.5), "Digital");
    assert!(c.param(&ParamAddr::new("rig/master_trim")).is_some());
    assert!(c.action(&EntryId::new("tap_tempo")).is_some());
    assert!(c.stream(&EntryId::new("out_level")).is_some());
    let featured: Vec<&str> = c.params.iter().filter(|p| p.featured).map(|p| p.addr.0.as_str()).collect();
    assert_eq!(featured, ["Delay/time", "Delay/on", "Delay 2/mix", "Delay 2/on", "Delay 2/type", "rig/master_trim"]);
}

#[test]
fn values_and_routing_round_trip_real_values() {
    let perf = PerformanceModel::default();
    let chain = chain();
    let c = guitar_catalog(&chain, &perf, &[], &ranges());
    let v = guitar_values(&chain, &perf, &c);
    let time = v[&ParamAddr::new("Delay/time")];
    // Log taper: 350 ms sits well above the linear position.
    assert!(time > (350.0 - 20.0) / 1980.0);
    match route_param(&c, &ParamAddr::new("Delay/time"), time).unwrap() {
        RigCall::Param { block, param, real } => {
            assert_eq!((block.as_str(), param.as_str()), ("Delay", "time"));
            assert!((real - 350.0).abs() < 0.01, "{real}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(v[&ParamAddr::new("Delay 2/type")], 0.5);
    assert_eq!(route_param(&c, &ParamAddr::new("Delay 2/type"), 1.0).unwrap(), RigCall::Option { block: "Delay 2".into(), option: 2 });
    assert_eq!(route_param(&c, &ParamAddr::new("Delay/on"), 0.0).unwrap(), RigCall::Bypass { block: "Delay".into(), bypassed: true });
    assert!(route_param(&c, &ParamAddr::new("Nope/x"), 0.5).is_err());
}

#[test]
fn actions_selects_and_telemetry() {
    assert_eq!(route_action(&EntryId::new("stack/3")).unwrap(), RigCall::Stack(3));
    assert!(route_action(&EntryId::new("launch")).is_err());
    assert_eq!(route_select(&EntryId::new("songs"), 2).unwrap(), RigCall::Song(2));
    let frames = status_frames(&RigStatus { output_peak_l: 0.5, output_peak_r: 1.0, comp_gr_db: 3.0, ..RigStatus::default() });
    let out = frames.iter().find(|f| f.id.0 == "out_level").unwrap();
    assert!((out.values[0] - -6.0206).abs() < 1e-3 && out.values[1].abs() < 1e-9);
    assert_eq!(frames.iter().find(|f| f.id.0 == "gain_reduction").unwrap().values, vec![-3.0]);
}

#[test]
fn a_frame_surface_generated_for_the_rig_is_valid() {
    let c = guitar_catalog(&chain(), &PerformanceModel::default(), &[], &ranges());
    for p in &c.params {
        assert!((0.0..=1.0).contains(&p.default), "{} default {}", p.addr, p.default);
    }
}
