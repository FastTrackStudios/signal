//! Talk to a live engine's `SurfaceEngine` and prove the round trip.
//!
//! ```text
//! engine_probe [--url ws://127.0.0.1:4040/vox] [--rig guitar] [--set ADDR=VALUE] [--catalog-out catalog.json]
//! ```
//! Prints the catalog (groups, params, actions, streams), sets one param
//! (normalized) and reads the value back through the engine.

use frame::surface::proto::{EditPhase, ParamAddr};
use frame::surface::runtime::EngineCall;
use frame::surface::runtime::sync::{Engine, connect_ws};

fn main() {
    let mut url = "ws://127.0.0.1:4040/vox".to_string();
    let mut rig = Some("guitar".to_string());
    let mut set: Option<(String, f64)> = None;
    let mut catalog_out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--url" => url = args.next().unwrap_or(url),
            "--rig" => rig = args.next().filter(|r| r != "none"),
            "--catalog-out" => catalog_out = args.next(),
            "--set" => {
                let kv = args.next().unwrap_or_default();
                set = kv
                    .split_once('=')
                    .and_then(|(k, v)| Some((k.to_string(), v.parse().ok()?)));
            }
            other => {
                eprintln!("engine_probe: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let result = rt.block_on(async {
        let engine = connect_ws(&url, rig.as_deref()).await?;
        let catalog = Engine::catalog(&engine).await?;
        println!(
            "engine `{}` v{}: {} params, {} actions, {} streams, {} collections",
            catalog.id,
            catalog.version,
            catalog.params.len(),
            catalog.actions.len(),
            catalog.streams.len(),
            catalog.collections.len()
        );
        for (group, params) in catalog.groups() {
            let names: Vec<String> = params
                .iter()
                .map(|p| p.addr.0.rsplit('/').next().unwrap_or_default().to_string())
                .collect();
            println!("  [{}] {}", group.unwrap_or("-"), names.join(", "));
        }
        if let Some(path) = &catalog_out {
            let json = serde_json::to_string_pretty(&catalog)
                .map_err(|e| frame::surface::runtime::sync::EngineError(e.to_string()))?;
            std::fs::write(path, json)
                .map_err(|e| frame::surface::runtime::sync::EngineError(format!("{path}: {e}")))?;
            println!("wrote catalog to {path}");
        }
        let values = Engine::values(&engine).await?;
        println!("{} values", values.len());
        if let Some((addr, value)) = set {
            let addr = ParamAddr::new(addr);
            let before = values.iter().find(|v| v.addr == addr).map(|v| v.value);
            Engine::call(
                &engine,
                EngineCall::SetParam {
                    addr: addr.clone(),
                    value,
                    phase: EditPhase::End,
                },
            )
            .await?;
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let after = Engine::values(&engine)
                .await?
                .into_iter()
                .find(|v| v.addr == addr)
                .map(|v| v.value);
            let info = catalog.param(&addr);
            println!(
                "set {addr} = {value}: before {} → after {}",
                before
                    .zip(info)
                    .map_or("?".into(), |(v, p)| format!("{v:.4} ({})", p.display(v))),
                after
                    .zip(info)
                    .map_or("?".into(), |(v, p)| format!("{v:.4} ({})", p.display(v))),
            );
        }
        Ok::<_, frame::surface::runtime::sync::EngineError>(())
    });
    if let Err(e) = result {
        eprintln!("engine_probe: {e}");
        std::process::exit(1);
    }
}
