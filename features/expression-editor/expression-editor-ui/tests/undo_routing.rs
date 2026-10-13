//! Undo reaches the thing that was actually edited.
//!
//! In drum mode the document is a projection of audio living on the
//! daw. A slip, a stretch and a quantize all write there, wrapped in the
//! host's own undo block, and none of them touch the document's history.
//! So the toolbar's undo — which rewound the *document* — had nothing to
//! rewind: it did nothing visible while the edit stayed on disk, which
//! is the worst shape a bug can take, because the user believes the take
//! is back the way it was.
//!
//! `DrumHost::undo` was tested from the start. What was never tested is
//! the path from the button to it, which is why nobody noticed there
//! wasn't one.

use std::cell::RefCell;

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};
use expression_editor_core::{Editor, ExpressionDoc, Note, NoteId, TimeBase, Viewport};
use expression_editor_ui::ExpressionEditor;

thread_local! {
    static STAGED: RefCell<Option<Editor>> = const { RefCell::new(None) };
    static HOST_UNDOS: RefCell<usize> = const { RefCell::new(0) };
    static WIRED: RefCell<bool> = const { RefCell::new(false) };
}

/// An editor with one note already deleted, so the document has
/// something on its own undo stack to rewind.
fn editor_with_history() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: 960.0 }, 0.0, 960.0 * 8.0);
    doc.push(Note::new(NoteId(1), 0.0, 480.0, 60));
    doc.push(Note::new(NoteId(2), 960.0, 1440.0, 62));
    let mut ed = Editor::new(doc, Viewport::new(1000.0, 400.0));
    ed.selection.set_single(NoteId(2));
    // Any undoable document edit will do; lengthening a note is the
    // least machinery.
    assert!(ed.nudge_note_lengths(120.0), "the nudge did nothing");
    assert!(ed.can_undo(), "the fixture must have something to undo");
    ed
}

/// The length of note 2 — what the document's own undo would restore.
fn length_of(ed: &Editor) -> f64 {
    ed.doc
        .note(NoteId(2))
        .map(|n| n.end - n.start)
        .unwrap_or_default()
}

#[component]
fn Surface() -> Element {
    let editor = use_signal(|| STAGED.with(|s| s.borrow_mut().take()).expect("staged"));
    let notes = length_of(&editor.read()) as usize;
    let on_undo = WIRED.with(|w| *w.borrow()).then(|| {
        EventHandler::new(move |()| {
            HOST_UNDOS.with(|c| *c.borrow_mut() += 1);
        })
    });
    rsx! {
        div { "data-testid": "readout", "notes={notes}" }
        ExpressionEditor { editor, on_undo }
    }
}

fn notes(html: &str) -> usize {
    html.split_whitespace()
        .find_map(|kv| kv.strip_prefix("notes=")?.parse().ok())
        .unwrap_or_else(|| panic!("no `notes` in readout: {html}"))
}

async fn click_undo(wired: bool, ed: Editor) -> dioxus_test::Result<(usize, usize)> {
    WIRED.with(|w| *w.borrow_mut() = wired);
    HOST_UNDOS.with(|c| *c.borrow_mut() = 0);
    STAGED.with(|s| *s.borrow_mut() = Some(ed));

    let tester = render(Surface).with_window_size(1400, 700).build();
    let button = tester.query(by_testid("undo")).immediately()?;
    let (ox, oy) = button.document_origin();
    let (w, h) = button.size();
    let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    tester.pointer_down_mods(x, y, dioxus_test::keyboard_types::Modifiers::empty());
    tester.pointer_up_mods(x, y, dioxus_test::keyboard_types::Modifiers::empty());
    let _ = tester.pump().await;

    let html = tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html();
    Ok((HOST_UNDOS.with(|c| *c.borrow()), notes(&html)))
}

// r[verify drums.manual.undo]
#[tokio::test]
async fn with_a_host_undo_goes_to_the_host() -> dioxus_test::Result<()> {
    let (host_undos, notes) = click_undo(true, editor_with_history()).await?;
    assert_eq!(host_undos, 1, "the host was never asked to undo");
    // And the document was left alone: rewinding it as well would undo
    // two different things for one click.
    assert_eq!(
        notes, 600,
        "the document was rewound too — one click, two undos"
    );
    Ok(())
}

// r[verify drums.manual.undo]
#[tokio::test]
async fn without_a_host_undo_still_rewinds_the_document() -> dioxus_test::Result<()> {
    // The piano roll and every demo scene have no host, and their edits
    // really are in the document. Routing must not cost them undo.
    let (host_undos, notes) = click_undo(false, editor_with_history()).await?;
    assert_eq!(host_undos, 0);
    assert_eq!(notes, 480, "the document edit was not rewound");
    Ok(())
}
