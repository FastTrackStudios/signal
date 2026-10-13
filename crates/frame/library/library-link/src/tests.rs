//! End to end: an `FsLibrary` served over a real WebSocket in-process, a
//! blocking client publishing and reading, and a follower receiving the
//! live updates — including an edit made on disk, a server restart, and
//! a switch to another surface.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use architect::Services as _;
use library_fs::FsLibrary;

use super::*;

const PANEL: &str =
    r##"<Document><Page><Frame name="Panel" w=200 h=100 fill="#101010" /></Page></Document>"##;

fn panel(name: &str) -> String {
    PANEL.replace("\"Panel\"", &format!("{name:?}"))
}

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("library-link-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The next event matching `want`, within 5 s.
fn wait_for(f: &Follow, want: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(e) = f.recv_timeout(Duration::from_millis(50))
            && want(&e)
        {
            return e;
        }
    }
    panic!("no matching event within 5 s");
}

fn name_of(e: &Event) -> Option<String> {
    match e {
        Event::Update(SurfaceUpdate {
            surface: Some(s), ..
        }) => s.content.decode().ok().and_then(|sc| scene_name(&sc)),
        _ => None,
    }
}

#[test]
fn server_in_process_client_subscribes() {
    let dir = Dir::new("e2e");
    std::fs::create_dir_all(dir.0.join("fx")).unwrap();
    let file = dir.0.join("fx/verb.frame");
    std::fs::write(&file, panel("Verb")).unwrap();
    let lib = FsLibrary::open(&dir.0).unwrap();
    let _watch = lib.watch_every(Duration::from_millis(20));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let served = Served::start(lib.clone().into_router(), listener).unwrap();

    // publish → list → get, blocking.
    let mut client = Blocking::new(&served.url).unwrap();
    let content = Content::markup_text(panel("Delay"));
    let p = client
        .publish(
            &content,
            &PublishMeta {
                id: "fx/delay".into(),
                author: "test".into(),
                ..PublishMeta::default()
            },
        )
        .unwrap();
    assert_eq!(
        p,
        Published {
            id: "fx/delay".into(),
            version: 1
        }
    );
    let ids: Vec<String> = client
        .list(&ListQuery::default())
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(ids, ["fx/delay", "fx/verb"]);
    let got = client.get("fx/delay", None).unwrap();
    assert_eq!((got.content, got.meta.author.as_str()), (content, "test"));
    let refused = client.get("fx/none", None).unwrap_err();
    assert!(refused.contains("NotFound"), "{refused}");
    let bad = client
        .publish(&Content::markup_text("<Document>"), &PublishMeta::default())
        .unwrap_err();
    assert!(bad.contains("BadContent"), "{bad}");

    // Follow: the snapshot, then an edit on disk.
    let follow = Follow::start(&served.url, Some("fx/verb".into())).unwrap();
    let first = wait_for(&follow, |e| matches!(e, Event::Update(_)));
    assert_eq!(name_of(&first).as_deref(), Some("Verb"));
    std::fs::write(&file, panel("Verb II")).unwrap();
    let e = wait_for(&follow, |e| matches!(e, Event::Update(_)));
    assert_eq!(name_of(&e).as_deref(), Some("Verb II"));
    let Event::Update(u) = e else { unreachable!() };
    assert_eq!(u.version, 2);
    // The index change for it arrives as well.
    wait_for(
        &follow,
        |e| matches!(e, Event::Index(c) if c.id == "fx/verb"),
    );

    // Switch to another surface: its snapshot arrives.
    follow.follow(Some("fx/delay".into()));
    let e = wait_for(
        &follow,
        |e| matches!(e, Event::Update(u) if u.id == "fx/delay"),
    );
    assert_eq!(name_of(&e).as_deref(), Some("Delay"));

    // Server gone → Lost; back → Connected and the snapshot again.
    let addr = served.addr().to_owned();
    drop(served);
    wait_for(&follow, |e| matches!(e, Event::Lost(_)));
    let served = Served::start(
        lib.clone().into_router(),
        std::net::TcpListener::bind(&addr).unwrap(),
    )
    .unwrap();
    wait_for(&follow, |e| matches!(e, Event::Connected));
    let e = wait_for(
        &follow,
        |e| matches!(e, Event::Update(u) if u.id == "fx/delay"),
    );
    assert_eq!(name_of(&e).as_deref(), Some("Delay"));

    // The blocking client recovers on a fresh link.
    assert_eq!(client.list(&ListQuery::default()).unwrap().len(), 2);
    drop(served);
}
