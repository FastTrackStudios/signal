//! Offline repro: encode the exact payloads `SetlistService.setlist()`
//! returns through the facet wire codec (facet-postcard, the 0.50-line
//! successor to the old `vox_postcard` re-export), with no network.
use facet::Facet;
use session_proto::services::SessionServiceError;
use session_proto::setlist::Setlist;
use session_proto::song::{Chart, Song};

// This is what the live RPC path does (and raw `to_vec` does NOT):
// build the wire schema registry for the return type. If this errors
// or loops, the client never sends the request → hang.
#[test]
fn schema_extracts_for_rpc_return_types() {
    for (name, shape) in [
        ("Setlist", <Setlist as Facet>::SHAPE),
        ("Song", <Song as Facet>::SHAPE),
        ("Chart", <Chart as Facet>::SHAPE),
        ("Vec<Song>", <Vec<Song> as Facet>::SHAPE),
        ("SessionServiceError", <SessionServiceError as Facet>::SHAPE),
    ] {
        match vox_types::extract_schemas(shape) {
            Ok(s) => eprintln!("{name}: OK ({} schemas)", s.schemas.len()),
            Err(e) => panic!("{name}: schema extraction FAILED: {e}"),
        }
    }
}

#[test]
fn empty_chart_roundtrips() {
    let c = Chart::new();
    let bytes = facet_postcard::to_vec(&c).expect("encode empty chart");
    eprintln!("empty chart encoded {} bytes", bytes.len());
    let back: Chart = facet_postcard::from_slice(&bytes).expect("decode empty chart");
    assert!(c == back, "empty chart mismatch after roundtrip");
}

#[test]
fn chart_with_section_memory_roundtrips() {
    use keyflow::sections::SectionType;
    let mut c = Chart::new();
    c.section_measure_memory
        .insert(SectionType::parse("Verse").unwrap(), 4);
    let bytes = facet_postcard::to_vec(&c).expect("encode chart w/ map");
    eprintln!("chart w/ map encoded {} bytes", bytes.len());
    let back: Chart = facet_postcard::from_slice(&bytes).expect("decode chart w/ map");
    assert!(c == back, "chart w/ map mismatch after roundtrip");
}

#[test]
fn error_roundtrips() {
    let e = SessionServiceError::not_found("Setlist", "current");
    let bytes = facet_postcard::to_vec(&e).expect("encode error");
    eprintln!("error encoded {} bytes", bytes.len());
    let back: SessionServiceError = facet_postcard::from_slice(&bytes).expect("decode error");
    assert_eq!(e, back);
}

#[test]
fn empty_setlist_roundtrips() {
    let s = Setlist::default();
    let bytes = facet_postcard::to_vec(&s).expect("encode empty setlist");
    eprintln!("empty setlist encoded {} bytes", bytes.len());
    let back: Setlist = facet_postcard::from_slice(&bytes).expect("decode empty setlist");
    assert_eq!(s, back);
}

#[test]
fn named_setlist_roundtrips() {
    let s = Setlist {
        id: Some("x".into()),
        name: "Demo".into(),
        ..Default::default()
    };
    let bytes = facet_postcard::to_vec(&s).expect("encode named setlist");
    eprintln!("named setlist encoded {} bytes", bytes.len());
    let back: Setlist = facet_postcard::from_slice(&bytes).expect("decode named setlist");
    assert_eq!(s, back);
}
