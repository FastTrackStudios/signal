//! The service against a stubbed catalog.
//!
//! The unit tests cover the parts that need no server (URL construction,
//! state checking, mapping). These cover the parts that only exist as a
//! conversation: redeeming an authorization, paging a search, merging a
//! tone's two architectures, and the download path from `model_url` to a
//! catalog entry carrying its licence.
//!
//! No network: `wiremock` binds a loopback port and `SIGNAL_T3K_BASE_URL`'s
//! config equivalent points the client at it.

use signal_tone3000_proto::tone3000::Tone3000 as _;
use signal_tone3000_proto::{ToneQuery, ToneShelf};
use signal_tone3000::{Config, Tone3000Backend};
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TONE_ID: &str = "51949";

/// A minimal but real `.nam` file — the scanner parses this, so the catalog
/// entry it produces is a real one rather than a stub.
const NAM_BYTES: &str = r#"{"version":"0.5.2","architecture":"WaveNet","sample_rate":48000,
    "metadata":{"gear_make":"Marshall","gear_model":"Plexi","tone_type":"crunch",
    "modeled_by":"brucew"},"weights":[0.1,0.2]}"#;

fn tone_detail_json() -> serde_json::Value {
    serde_json::json!({
        "id": 51949, "user_id": "57af", "title": "Plexi 51",
        "description": "1968 Super Lead", "gear": "amp", "format": "nam",
        "license": "cc-by",
        "makes": [{"name": "Marshall Plexi"}], "tags": [{"name": "crunch"}],
        "images": ["IMAGE_URL"],
        "user": {"id": "57af", "username": "brucew", "url": "https://t/u/brucew"},
        "url": "https://www.tone3000.com/tones/51949",
        "models_count": 3, "a1_models_count": 1, "a2_models_count": 1,
        "custom_models_count": 0, "downloads_count": 900, "favorites_count": 12
    })
}

/// Stand up the mock catalog and a backend pointed at it, in a temp library.
async fn fixture() -> (MockServer, Tone3000Backend, tempfile::TempDir) {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        publishable_key: "t3k_pub_test".into(),
        redirect_uri: "http://127.0.0.1:4040/tone3000/callback".into(),
        base_url: Some(format!("{}/api/v1", server.uri())),
        library_root: dir.path().join("nam"),
        catalog_path: dir.path().join("nam/catalog.json"),
        token_path: dir.path().join("session.json"),
        image_cache: dir.path().join("images"),
    };
    (server, Tone3000Backend::new(cfg), dir)
}

/// Mount the token + user stubs and drive a full sign-in, returning the
/// `request_id` so a caller can ask what tone the picker came back with.
async fn sign_in(server: &MockServer, backend: &Tone3000Backend, with_tone: bool) -> String {
    Mock::given(method("POST"))
        .and(path("/api/v1/oauth/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "at_1", "refresh_token": "rt_1",
            "token_type": "bearer", "expires_in": 3600
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "57af", "username": "brucew", "url": "https://t/u/brucew",
            "is_verified": true, "created_at": "2024-01-01", "updated_at": "2024-01-01"
        })))
        .mount(server)
        .await;

    let request = backend.begin_sign_in(true);
    // The nonce is only knowable from the URL the engine built — which is
    // exactly the position the browser is in.
    let url = url::Url::parse(&request.authorize_url).expect("authorize url parses");
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .expect("state is in the authorize url");

    let tone = if with_tone { format!("&tone_id={TONE_ID}") } else { String::new() };
    let callback =
        format!("http://127.0.0.1:4040/tone3000/callback?code=auth_code&state={state}{tone}");
    let status = backend
        .complete_sign_in(request.request_id.clone(), callback)
        .await;
    assert!(status.signed_in, "sign-in failed: {}", status.error);
    assert_eq!(status.username, "brucew");
    request.request_id
}

#[tokio::test]
async fn signing_in_persists_the_session_and_the_account_name() {
    let (server, backend, _dir) = fixture().await;
    sign_in(&server, &backend, false).await;

    // Status must be answerable from disk alone — no request, and correct
    // even with the catalog unreachable.
    drop(server);
    let status = backend.status();
    assert!(status.signed_in);
    assert_eq!(status.username, "brucew");
    assert!(status.error.is_empty());
}

#[tokio::test]
async fn signing_out_forgets_the_session() {
    let (server, backend, _dir) = fixture().await;
    sign_in(&server, &backend, false).await;
    backend.sign_out();
    assert!(!backend.status().signed_in);
}

#[tokio::test]
async fn a_search_pages_and_maps_the_catalog() {
    let (server, backend, _dir) = fixture().await;
    sign_in(&server, &backend, false).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/tones/search"))
        .and(query_param("query", "plexi"))
        .and(query_param("gears", "amp"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [tone_detail_json()],
            "page": 1, "page_size": 24, "total": 1, "total_pages": 1
        })))
        .mount(&server)
        .await;

    let page = backend
        .search(ToneQuery {
            text: "plexi".into(),
            gears: vec!["amp".into()],
            ..ToneQuery::default()
        })
        .await;

    assert!(page.error.is_empty(), "{}", page.error);
    assert_eq!(page.total, 1);
    let row = page.tones.first().expect("one row");
    assert_eq!(row.title, "Plexi 51");
    assert_eq!(row.creator, "brucew");
    assert_eq!(row.makes, vec!["Marshall Plexi".to_string()]);
}

#[tokio::test]
async fn a_failed_search_says_so_instead_of_looking_empty() {
    let (server, backend, _dir) = fixture().await;
    sign_in(&server, &backend, false).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tones/search"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let page = backend.search(ToneQuery::default()).await;
    assert!(page.tones.is_empty());
    assert!(!page.error.is_empty(), "a rate limit must not read as no results");
}

/// The models endpoint serves one architecture per call. A tone with both
/// must come back with both — this is the bug the upstream docs warn about,
/// pinned here.
#[tokio::test]
async fn a_tone_carries_the_models_of_every_architecture_it_reports() {
    let (server, backend, _dir) = fixture().await;
    sign_in(&server, &backend, false).await;
    mount_tone(&server).await;

    let tone = backend.tone(TONE_ID.into()).await;
    assert!(tone.error.is_empty(), "{}", tone.error);
    assert_eq!(tone.license, "cc-by");
    assert_eq!(tone.creator, "brucew");
    assert_eq!(tone.models.len(), 2, "one v1 and one v2 model");
    let architectures: Vec<&str> = tone.models.iter().map(|m| m.architecture.as_str()).collect();
    assert!(architectures.contains(&"1") && architectures.contains(&"2"));
}

#[tokio::test]
async fn the_picker_hands_back_the_tone_the_user_chose() {
    let (server, backend, _dir) = fixture().await;
    let request_id = sign_in(&server, &backend, true).await;
    mount_tone(&server).await;

    let picked = backend.picked_tone(request_id).await;
    assert_eq!(picked.id, TONE_ID);
    assert_eq!(picked.name, "Plexi 51");
}

/// The whole point of the integration: a model reaches the library, and the
/// catalog entry carries the attribution the API terms require.
#[tokio::test]
async fn a_downloaded_model_lands_in_the_library_with_its_provenance() {
    let (server, backend, dir) = fixture().await;
    sign_in(&server, &backend, false).await;
    mount_tone(&server).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/models/1001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json(1001, "1", &server.uri())))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/plexi_di.nam"))
        .respond_with(ResponseTemplate::new(200).set_body_string(NAM_BYTES))
        .mount(&server)
        .await;

    backend.download_model(TONE_ID.into(), "1001".into());

    let placed = dir.path().join("nam/tone3000/51949/plexi_di.nam");
    wait_for(&placed).await;
    assert_eq!(
        std::fs::read_to_string(&placed).expect("placed file"),
        NAM_BYTES
    );

    let catalog_path = dir.path().join("nam/catalog.json");
    wait_for(&catalog_path).await;
    let catalog = signal_nam::NamCatalog::load(&catalog_path).expect("catalog loads");
    let entry = catalog
        .entries
        .values()
        .find(|e| e.filename == "plexi_di.nam")
        .expect("the download is in the catalog");
    let provenance = entry.provenance.as_ref().expect("provenance recorded");
    assert_eq!(provenance.source, "tone3000");
    assert_eq!(provenance.tone_id.as_deref(), Some(TONE_ID));
    assert_eq!(provenance.model_id.as_deref(), Some("1001"));
    assert_eq!(provenance.creator.as_deref(), Some("brucew"));
    assert_eq!(provenance.license.as_deref(), Some("cc-by"));
    assert_eq!(
        provenance.tone_url.as_deref(),
        Some("https://www.tone3000.com/tones/51949")
    );
    // The file's own metadata is indexed too — a downloaded capture is a
    // first-class library file, not a special case.
    assert_eq!(entry.gear_make.as_deref(), Some("Marshall"));
}

#[tokio::test]
async fn an_image_is_fetched_once_and_then_served_from_the_cache() {
    let (server, backend, dir) = fixture().await;
    sign_in(&server, &backend, false).await;
    mount_tone(&server).await;

    // One-byte-perfect PNG header is enough: the type is sniffed, not parsed.
    let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
    Mock::given(method("GET"))
        .and(path("/img/a.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png.clone()))
        .expect(1) // fetched exactly once, however often it is asked for
        .mount(&server)
        .await;

    // The URL only becomes fetchable because a tone carried it.
    let tone = backend.tone(TONE_ID.into()).await;
    let url = tone.images.first().cloned().expect("the tone has an image");

    let first = backend.image(url.clone()).await;
    assert!(first.error.is_empty(), "{}", first.error);
    assert_eq!(first.mime, "image/png");
    assert_eq!(first.bytes, png);

    let second = backend.image(url).await;
    assert_eq!(second.bytes, png, "the cache serves the same bytes");
    assert!(dir.path().join("images").is_dir(), "cached to disk");
}

#[tokio::test]
async fn a_shelf_fills_a_screen_without_a_query() {
    let (server, backend, _dir) = fixture().await;
    sign_in(&server, &backend, false).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tones/search"))
        .and(query_param("sort", "trending"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [tone_detail_json()],
            "page": 1, "page_size": 10, "total": 1, "total_pages": 1
        })))
        .mount(&server)
        .await;

    let page = backend.shelf(ToneShelf::Trending, 1).await;
    assert!(page.error.is_empty(), "{}", page.error);
    assert_eq!(page.tones.len(), 1);
}

/// Every call needs a session. Without one the answer must name that, so a
/// UI can offer sign-in rather than showing an empty catalog.
#[tokio::test]
async fn calls_without_a_session_report_that_and_not_emptiness() {
    let (_server, backend, _dir) = fixture().await;
    let page = backend.search(ToneQuery::default()).await;
    assert!(page.error.contains("not signed in"), "{}", page.error);
    let tone = backend.tone(TONE_ID.into()).await;
    assert!(tone.error.contains("not signed in"), "{}", tone.error);
}

async fn mount_tone(server: &MockServer) {
    let mut detail = tone_detail_json();
    detail["images"] = serde_json::json!([format!("{}/img/a.png", server.uri())]);

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/tones/{TONE_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(detail))
        .mount(server)
        .await;
    for (arch, id) in [("1", 1001), ("2", 1002)] {
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .and(query_param("architecture", arch))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [model_json(id, arch, &server.uri())],
                "page": 1, "page_size": 300, "total": 1, "total_pages": 1
            })))
            .mount(server)
            .await;
    }
}

fn model_json(id: u64, architecture: &str, base: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "tone_id": 51949, "user_id": "57af",
        "name": "Plexi 51 DI", "model_url": format!("{base}/files/plexi_di.nam"),
        "size": "standard", "architecture_version": architecture
    })
}

/// Poll for a file the queued download will produce. `download_model`
/// returns as soon as the work is spawned — that is the contract — so a test
/// waits for the effect rather than the call.
async fn wait_for(path: &std::path::Path) {
    for _ in 0..100 {
        if path.exists() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {}", path.display());
}
