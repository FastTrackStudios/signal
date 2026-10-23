//! Signal's command core over MCP, end to end: a design-mode rig over a
//! throwaway library, its Signal Action List served the way
//! `studio-desktop --engine --mcp` serves it (`serve_mcp`: loopback, a
//! minted bearer token), and a model's `run` of `FTS_SIGNAL_NEXT_SONG`
//! over HTTP moving the active song. Without the token the server refuses;
//! a REAPER-only command answers disabled, with why.

use guitar::GuitarRigBackend;
use guitar::proto::rig::Rig;
use guitar::signal_commands::{REAPER_ONLY, command_list, mint_token, serve_mcp};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// One `POST /mcp` to `addr`, raw HTTP/1.1: the status code and the body.
async fn post(addr: std::net::SocketAddr, body: &str, token: Option<&str>) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let auth = token.map_or_else(String::new, |t| format!("Authorization: Bearer {t}\r\n"));
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\n{auth}Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.expect("send");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.expect("read");
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("a status line");
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or_default();
    (status, body)
}

/// A `tools/call` of `run` for `action`, as a model sends it.
fn run_request(id: u64, action: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": "run", "arguments": {"id": action, "args": {}}},
    })
    .to_string()
}

/// A rig with a setlist of at least two songs up, on its first song.
fn rig_on_a_setlist() -> GuitarRigBackend {
    let root = std::env::temp_dir().join(format!("signal-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the only test in this binary, before any thread reads the env.
    unsafe {
        std::env::set_var("SIGNAL_RIG_DIR", root.join("rig"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
    }
    let rig = GuitarRigBackend::new();
    rig.open_blocking();
    Rig::set_perform_mode(&rig, 2);
    let sets = Rig::perf(&rig).setlists.len();
    let set = (0..sets)
        .find(|&i| Rig::select_setlist(&rig, i as u32).ok && Rig::perf(&rig).songs.len() >= 2)
        .expect("a shipped setlist with two songs");
    let applied = Rig::select_song(&rig, 0);
    assert!(applied.ok, "setlist {set}: {}", applied.message);
    rig
}

#[tokio::test(flavor = "multi_thread")]
async fn an_mcp_run_of_next_song_moves_the_active_song() {
    let rig = rig_on_a_setlist();
    let list = command_list(&rig).expect("Signal's commands register");
    list.validate().expect("the list is sound");
    assert_eq!(list.len(), 43);

    let token = mint_token();
    let serving = serve_mcp(list, 0, token.clone()).await.expect("served");
    assert!(serving.addr.ip().is_loopback(), "{}", serving.addr);

    let (status, _) = post(serving.addr, &run_request(1, "FTS_SIGNAL_NEXT_SONG"), None).await;
    assert_eq!(status, 401, "no token, no run");
    assert_eq!(Rig::perf(&rig).song_index, 0, "a refused run moved nothing");

    let (status, body) = post(
        serving.addr,
        &run_request(2, "FTS_SIGNAL_NEXT_SONG"),
        Some(&token),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let reply: Value = serde_json::from_str(&body).expect("a JSON-RPC reply");
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let perf = Rig::perf(&rig);
    assert_eq!(perf.song_index, 1, "next song moved the active song");
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let out: Value = serde_json::from_str(text).expect("the outcome is JSON");
    assert_eq!(
        out["summary"],
        format!("song 2/{}: {}", perf.songs.len(), perf.songs[1].name)
    );

    let (_, body) = post(
        serving.addr,
        &run_request(3, "FTS_SIGNAL_MACRO_ARM"),
        Some(&token),
    )
    .await;
    let reply: Value = serde_json::from_str(&body).expect("a JSON-RPC reply");
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert!(body.contains(REAPER_ONLY), "{body}");

    serving.task.abort();
}
