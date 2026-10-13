//! Throwaway probe: dump the Basic Pitch model's input/output facts so we can
//! build the front-end against the real graph instead of guessing. Run with:
//!   cargo test -p tune-dsp --features basic-pitch --test basic_pitch_probe -- --nocapture
#![cfg(feature = "basic-pitch")]

use tract_onnx::prelude::*;

#[test]
fn dump_model_io() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/basic_pitch/nmp.onnx");
    let model = tract_onnx::onnx().model_for_path(path).expect("load onnx");

    println!("=== INPUTS ===");
    for (i, id) in model.input_outlets().unwrap().iter().enumerate() {
        let name = model.node(id.node).name.clone();
        let fact = model.outlet_fact(*id).unwrap();
        println!("input[{i}] node='{name}' fact={fact:?}");
    }
    println!("=== OUTPUTS ===");
    for (i, id) in model.output_outlets().unwrap().iter().enumerate() {
        let name = model.node(id.node).name.clone();
        let fact = model.outlet_fact(*id).unwrap();
        println!("output[{i}] node='{name}' fact={fact:?}");
    }
}
