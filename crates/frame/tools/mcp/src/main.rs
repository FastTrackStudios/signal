//! `frame-mcp` — an MCP server (stdio) for designing control surfaces.
//!
//! ```text
//! frame-mcp [--file surface.frame] [--engine ws://127.0.0.1:4040/vox [--rig guitar]]
//!           [--library ws://127.0.0.1:4050/vox [--surface ID]]
//! ```
//! Serves the `design-session` tool registry over MCP. With `--file`, the
//! document is loaded from (and, after every edit, atomically saved to) a
//! frame-markup file, its textures and fonts in `<file>.assets/` (see
//! `document-io`) — point the `frame` desktop player at the same file and
//! it hot-reloads as the model works.
//!
//! With `--library`, every edit is also published to a surface library
//! (`frame-library-server`) as a new version of `--surface ID` (default:
//! the `--file` name, else a fresh id from the design's name), so every
//! player following it (`frame --library … --surface ID`) swaps it in
//! live. Without `--file`, the session starts from that surface's current
//! version in the library — how you edit a fork.
//!
//! The catalog designed against is the built-in demo rig's, or with
//! `--engine` a live engine's (fetched at startup; `--rig` picks the
//! instance scope). The extra `refresh_catalog` tool re-fetches it after
//! the engine's rig changes.
//!
//! Register with Claude Code:
//! `claude mcp add frame -- /path/to/frame-mcp --file /path/to/surface.frame`

use std::io::{BufRead, Write};
use std::path::PathBuf;

use design_session::{Session, tools};
use serde_json::{Value, json};
use surface_proto::{Catalog, SurfaceEngineClient};
use surface_runtime::sync;

const REFERENCE: &str = include_str!("../../../document/document-text/REFERENCE.md");

const INSTRUCTIONS: &str = "frame designs live-performance control surfaces (knobs, faders, footswitches, meters) \
that are bound to an audio engine's parameters. Workflow: `catalog` to see what can be controlled → `describe` \
to see the document → build with `generate` (full starting surface), `add_control` (canonical themed controls), \
or `write` (frame markup; call `reference` for the syntax) → `lint` and fix every error → iterate. Every tool \
call is one undoable step. Bind only to catalog entries (param:<addr>, action:<id>, stream:<id>[:channel]). \
Letter text with real fonts: `fonts` lists the bundled families (Inter, Oswald, Saira, Michroma, Orbitron, \
Dancing Script, Pacifico, Caveat Brush, Knewave, Playfair Display, DSEG7/DSEG14 Classic, Doto, VT323, JetBrains Mono, \
UnifrakturCook, Pirata One); set them with `font`, `weight`, `style` and `stretch` on <Text>.";

const USAGE: &str =
    "usage: frame-mcp [--file surface.frame] [--engine ws://HOST:PORT/vox [--rig NAME]] [--library ws://HOST:PORT/vox [--surface ID]]";

/// Where edits are published (`--library`).
struct Publish {
    client: library_link::Blocking,
    /// The surface id; `None` until the library assigns one.
    id: Option<String>,
}

impl Publish {
    /// Publish the session's document as the next version.
    fn publish(&mut self, session: &Session) -> Result<String, String> {
        let scene = session.scene();
        let meta = library_link::PublishMeta {
            id: self.id.clone().unwrap_or_default(),
            name: library_link::scene_name(scene).unwrap_or_default(),
            author: std::env::var("USER").unwrap_or_default(),
            ..library_link::PublishMeta::default()
        };
        let p = self.client.publish(&library_link::Content::markup(scene), &meta)?;
        self.id = Some(p.id.clone());
        Ok(format!("{}@{}", p.id, p.version))
    }
}

/// Where a fresh catalog comes from (`refresh_catalog`).
trait CatalogSource {
    /// Fetch the engine's current catalog.
    ///
    /// # Errors
    /// The engine can't be reached.
    fn fetch(&mut self) -> Result<Catalog, String>;
    /// Where it comes from, for messages.
    fn describe(&self) -> String;
}

/// A live engine over a WebSocket. Owns a small tokio runtime so the
/// stdio loop stays synchronous; the link is (re)opened on demand.
struct LiveEngine {
    rt: tokio::runtime::Runtime,
    url: String,
    rig: Option<String>,
    client: Option<SurfaceEngineClient>,
}

impl LiveEngine {
    fn new(url: String, rig: Option<String>) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!("tokio: {e}"))?;
        Ok(Self { rt, url, rig, client: None })
    }
}

impl CatalogSource for LiveEngine {
    fn fetch(&mut self) -> Result<Catalog, String> {
        let (url, rig) = (self.url.clone(), self.rig.clone());
        let cached = self.client.take();
        let (client, result) = self.rt.block_on(async move {
            // A cached link may have died with an engine restart: one retry
            // on a fresh link.
            if let Some(client) = cached
                && let Ok(catalog) = client.catalog().await
            {
                return (Some(client), Ok(catalog));
            }
            match sync::connect_ws(&url, rig.as_deref()).await {
                Ok(client) => match client.catalog().await {
                    Ok(catalog) => (Some(client), Ok(catalog)),
                    Err(e) => (None, Err(e.to_string())),
                },
                Err(e) => (None, Err(e.to_string())),
            }
        });
        self.client = client;
        result
    }

    fn describe(&self) -> String {
        match &self.rig {
            Some(rig) => format!("{} (rig {rig})", self.url),
            None => self.url.clone(),
        }
    }
}

fn main() {
    let mut file: Option<PathBuf> = None;
    let mut url: Option<String> = None;
    let mut rig: Option<String> = None;
    let mut library: Option<String> = None;
    let mut surface: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--file" => file = args.next().map(PathBuf::from),
            "--engine" => url = args.next(),
            "--rig" => rig = args.next(),
            "--library" => library = args.next(),
            "--surface" => surface = args.next(),
            "-h" | "--help" => {
                eprintln!("{USAGE}");
                return;
            }
            other => {
                eprintln!("frame-mcp: unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if rig.is_some() && url.is_none() {
        eprintln!("frame-mcp: --rig needs --engine\n{USAGE}");
        std::process::exit(2);
    }
    if surface.is_some() && library.is_none() {
        eprintln!("frame-mcp: --surface needs --library\n{USAGE}");
        std::process::exit(2);
    }
    let mut publish = match library.as_deref().map(library_link::Blocking::new) {
        None => None,
        Some(Ok(client)) => {
            let id = surface.clone().or_else(|| file.as_ref().and_then(|f| f.file_stem()).map(|s| s.to_string_lossy().into_owned()));
            eprintln!("frame-mcp: publishing every edit to {} as {}", client.url(), id.as_deref().unwrap_or("a new surface"));
            Some(Publish { client, id })
        }
        Some(Err(e)) => {
            eprintln!("frame-mcp: {e}");
            std::process::exit(1);
        }
    };
    let mut engine = match url.map(|u| LiveEngine::new(u, rig)) {
        None => None,
        Some(Ok(e)) => Some(e),
        Some(Err(e)) => {
            eprintln!("frame-mcp: {e}");
            std::process::exit(1);
        }
    };
    let catalog = match &mut engine {
        None => surface_memory::demo_catalog(),
        Some(e) => match e.fetch() {
            Ok(c) => {
                eprintln!("frame-mcp: catalog `{}` from {} ({} params)", c.id, e.describe(), c.params.len());
                c
            }
            Err(err) => {
                eprintln!("frame-mcp: can't fetch the catalog from {}: {err}", e.describe());
                std::process::exit(1);
            }
        },
    };
    let mut session = match file.as_ref().filter(|p| p.exists()) {
        // `.frame` (or legacy `.fm`) with its `.frame.assets/` textures and fonts, or a `.frame` bundle.
        Some(p) => match Session::open(p, catalog) {
            Ok(session) => session,
            Err(e) => {
                eprintln!("frame-mcp: can't load {}: {e}", p.display());
                std::process::exit(1);
            }
        },
        None => match (&mut publish, &surface) {
            // No file: start from the library's current version.
            (Some(p), Some(id)) => match p.client.get(id, None).and_then(|s| s.content.decode()) {
                Ok(scene) => {
                    eprintln!("frame-mcp: editing {id} from {}", p.client.url());
                    Session::new(scene, catalog)
                }
                Err(e) => {
                    eprintln!("frame-mcp: {id} isn't in the library yet ({e}); starting blank");
                    Session::blank(catalog)
                }
            },
            _ => Session::blank(catalog),
        },
    };

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(
                &mut session,
                Sinks { file: file.as_ref(), library: publish.as_mut() },
                engine.as_mut().map(|e| e as &mut dyn CatalogSource),
                &msg,
            ),
            Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}})),
        };
        if let Some(r) = reply {
            let _ = writeln!(stdout, "{r}");
            let _ = stdout.flush();
        }
    }
}

/// `refresh_catalog`: re-fetch and swap the session's catalog.
fn refresh_catalog(session: &mut Session, engine: Option<&mut dyn CatalogSource>) -> (String, bool) {
    let Some(engine) = engine else {
        return ("no live engine: frame-mcp runs against the built-in demo catalog (start it with --engine ws://…)".into(), true);
    };
    match engine.fetch() {
        Ok(c) => {
            let text = format!(
                "catalog `{}` v{} from {}: {} params, {} actions, {} streams, {} collections",
                c.id,
                c.version,
                engine.describe(),
                c.params.len(),
                c.actions.len(),
                c.streams.len(),
                c.collections.len()
            );
            session.set_catalog(c);
            let lint = session.call("lint", &json!({})).unwrap_or_else(|e| e.0);
            (format!("{text}\nlint against it: {lint}"), false)
        }
        Err(e) => (format!("refreshing the catalog from {} failed (the old catalog stays): {e}", engine.describe()), true),
    }
}

/// Where a mutating tool call's result goes.
#[derive(Default)]
struct Sinks<'a> {
    file: Option<&'a PathBuf>,
    library: Option<&'a mut Publish>,
}

/// One JSON-RPC message → optional reply (notifications get none).
fn handle(session: &mut Session, sinks: Sinks<'_>, engine: Option<&mut dyn CatalogSource>, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?; // notifications carry no id
    let method = msg.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2025-06-18")),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "frame", "version": env!("CARGO_PKG_VERSION")},
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut list: Vec<Value> = tools()
                .into_iter()
                .map(|t| json!({"name": t.name, "description": t.description, "inputSchema": t.input_schema}))
                .collect();
            list.push(json!({
                "name": "reference",
                "description": "The frame markup authoring reference (elements, attributes, fonts, examples). Read before using `write`/`replace`.",
                "inputSchema": {"type": "object", "properties": {}},
            }));
            if engine.is_some() {
                list.push(json!({
                    "name": "refresh_catalog",
                    "description": "Re-fetch the catalog from the live engine (after its rig changed), then lint the document against it.",
                    "inputSchema": {"type": "object", "properties": {}},
                }));
            }
            Ok(json!({"tools": list}))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let (text, is_error) = if name == "reference" {
                (REFERENCE.to_string(), false)
            } else if name == "refresh_catalog" {
                refresh_catalog(session, engine)
            } else {
                match session.call(name, &args) {
                    Ok(mut text) => {
                        let mutates = tools().iter().any(|t| t.name == name && t.mutates);
                        if mutates && let Some(path) = sinks.file
                            && let Err(e) = session.save(path)
                        {
                            text += &format!("\n(warning: saving {} failed: {e})", path.display());
                        }
                        if mutates && let Some(library) = sinks.library {
                            match library.publish(session) {
                                Ok(at) => text += &format!("\n(published {at})"),
                                Err(e) => text += &format!("\n(warning: publishing to the library failed: {e})"),
                            }
                        }
                        (text, false)
                    }
                    Err(e) => (e.0, true),
                }
            };
            Ok(json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
        }
        other => Err(json!({"code": -32601, "message": format!("method not found: {other}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rpc(s: &mut Session, method: &str, params: Value) -> Value {
        handle(s, Sinks::default(), None, &json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})).unwrap()
    }

    fn rpc_with(s: &mut Session, engine: &mut dyn CatalogSource, method: &str, params: Value) -> Value {
        handle(s, Sinks::default(), Some(engine), &json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})).unwrap()
    }

    fn tool_names(list: &Value) -> Vec<String> {
        list["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str().map(String::from)).collect()
    }

    /// An engine whose catalog changes between fetches.
    struct Scripted(Vec<Result<Catalog, String>>);

    impl CatalogSource for Scripted {
        fn fetch(&mut self) -> Result<Catalog, String> {
            self.0.remove(0)
        }
        fn describe(&self) -> String {
            "scripted".into()
        }
    }

    #[test]
    fn refresh_catalog_swaps_the_catalog() {
        let mut smaller = surface_memory::demo_catalog();
        smaller.id = "live.guitar".into();
        smaller.params.retain(|p| !p.addr.0.starts_with("delay/"));
        let mut engine = Scripted(vec![Ok(smaller), Err("connection refused".into())]);
        let mut s = Session::blank(surface_memory::demo_catalog());
        assert!(tool_names(&rpc_with(&mut s, &mut engine, "tools/list", json!({}))).contains(&"refresh_catalog".into()));
        rpc(&mut s, "tools/call", json!({"name": "generate", "arguments": {}}));

        let r = rpc_with(&mut s, &mut engine, "tools/call", json!({"name": "refresh_catalog", "arguments": {}}));
        assert_eq!(r["result"]["isError"], false);
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("`live.guitar`") && text.contains("lint"), "{text}");
        assert_eq!(s.catalog().id, "live.guitar");

        let r = rpc_with(&mut s, &mut engine, "tools/call", json!({"name": "refresh_catalog", "arguments": {}}));
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("connection refused"));
        assert_eq!(s.catalog().id, "live.guitar", "a failed refresh keeps the old catalog");
    }

    #[test]
    fn refresh_catalog_without_an_engine() {
        let mut s = Session::blank(surface_memory::demo_catalog());
        assert!(!tool_names(&rpc(&mut s, "tools/list", json!({}))).contains(&"refresh_catalog".into()));
        let r = rpc(&mut s, "tools/call", json!({"name": "refresh_catalog", "arguments": {}}));
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(s.catalog().id, "demo.guitar");
    }

    #[test]
    fn live_engine_fetches_over_a_websocket() {
        use architect::{LayerRouter, Services as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("ws://{}/vox", listener.local_addr().unwrap());
        let router = LayerRouter::new().merge_router_scoped("guitar", surface_memory::Memory::demo().into_router());
        // Serves until the test process exits.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move { sync::serve_ws(tokio::net::TcpListener::from_std(listener).unwrap(), router).await });
        });
        let mut engine = LiveEngine::new(url, Some("guitar".into())).unwrap();
        assert_eq!(engine.fetch().unwrap().id, "demo.guitar");
        assert!(engine.client.is_some(), "link kept for the next refresh");
        assert_eq!(engine.fetch().unwrap().id, "demo.guitar", "reuses the link");
        let mut s = Session::blank(Catalog::default());
        let r = rpc_with(&mut s, &mut engine, "tools/call", json!({"name": "refresh_catalog", "arguments": {}}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        assert_eq!(s.catalog().id, "demo.guitar");
    }

    #[test]
    fn mcp_handshake_list_and_call() {
        let mut s = Session::blank(surface_memory::demo_catalog());
        let init = rpc(&mut s, "initialize", json!({"protocolVersion": "2025-06-18"}));
        assert_eq!(init["result"]["serverInfo"]["name"], "frame");
        assert!(handle(&mut s, Sinks::default(), None, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
        let list = rpc(&mut s, "tools/list", json!({}));
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"generate") && names.contains(&"reference") && names.contains(&"lint"));
        let r = rpc(&mut s, "tools/call", json!({"name": "generate", "arguments": {}}));
        assert_eq!(r["result"]["isError"], false);
        let r = rpc(&mut s, "tools/call", json!({"name": "bind", "arguments": {"node": "nope"}}));
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(rpc(&mut s, "bogus", json!({}))["error"]["code"], -32601);
    }

    #[test]
    fn edits_publish_to_the_library() {
        use architect::Services as _;
        use library_fs::SurfaceStore as _;
        let dir = std::env::temp_dir().join(format!("frame-mcp-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lib = library_fs::FsLibrary::open(&dir).unwrap();
        let served = library_link::Served::start(lib.clone().into_router(), std::net::TcpListener::bind("127.0.0.1:0").unwrap()).unwrap();
        let mut publish = Publish { client: library_link::Blocking::new(&served.url).unwrap(), id: Some("agent/one".into()) };
        let mut s = Session::blank(surface_memory::demo_catalog());
        let call = |s: &mut Session, p: &mut Publish, name: &str| {
            let msg = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": {}}});
            handle(s, Sinks { file: None, library: Some(p) }, None, &msg).unwrap()
        };
        let r = call(&mut s, &mut publish, "generate");
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().ends_with("(published agent/one@1)"), "{r}");
        let stored = lib.get("agent/one".into(), None).unwrap();
        assert_eq!(stored.content.decode().unwrap(), *s.scene());
        // Read-only tools don't publish.
        let r = call(&mut s, &mut publish, "lint");
        assert!(!r["result"]["content"][0]["text"].as_str().unwrap().contains("published"), "{r}");
        assert_eq!(lib.get("agent/one".into(), None).unwrap().meta.version, 1);
        drop(served);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn edits_save_to_the_file() {
        let dir = std::env::temp_dir().join(format!("frame-mcp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.frame");
        let mut s = Session::blank(surface_memory::demo_catalog());
        handle(&mut s, Sinks { file: Some(&path), library: None }, None, &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "generate", "arguments": {}}}));
        assert_eq!(document_io::load_surface(&path).unwrap(), *s.scene());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_textured_file_keeps_its_textures_across_edits() {
        let dir = std::env::temp_dir().join(format!("frame-mcp-assets-{}", std::process::id()));
        let path = dir.join("tex.frame");
        let mut scene = Session::blank(surface_memory::demo_catalog()).scene().clone();
        scene.add_asset("image/png", b"texture".to_vec());
        document_io::save_surface(&path, &scene).unwrap();

        let mut s = Session::open(&path, surface_memory::demo_catalog()).unwrap();
        let page = s.scene().pages()[0].to_string();
        let msg = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "write", "arguments": {"parent": page, "markup": "<Frame name=\"New\" w=10 h=10 />"}}});
        handle(&mut s, Sinks { file: Some(&path), library: None }, None, &msg);
        let back = document_io::load_surface(&path).unwrap();
        assert!(back.nodes.values().any(|n| n.name == "New"), "the edit was saved");
        assert_eq!(back.assets, scene.assets, "…with the textures");
        let _ = std::fs::remove_dir_all(dir);
    }
}
