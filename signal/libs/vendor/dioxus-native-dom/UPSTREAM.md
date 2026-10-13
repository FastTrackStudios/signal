Source: DioxusLabs/dioxus, revision f717a8e184a522d078b70bb4b4d62a5f9a99ddfc,
packages/native-dom (0.8.0-alpha.0). Workspace dependency declarations are expanded
without changing their versions. License: MIT OR Apache-2.0.

Session's combined workstation DOM stress exposed stale ElementId-to-NodeId
mappings after subtree removal. Blitz reuses numeric slab IDs. assign_node_id's
cleanup of an old unparented mapping could therefore delete a fresh template
clone, then panic in node_at_path. Clear mappings for removed/replaced subtrees
before immediately dropping the removed subtree, using a reverse map to avoid scanning the whole document.
Queued mount events skip elements removed before delivery.

Remove this patch after upgrading to an upstream version with equivalent mapping
lifetime guarantees. Regression coverage lives in Session's DOM stress harness
and the native mutation writer unit tests.

Additionally (from session, 2026-09): `set_focus` never borrows a document
someone else holds. It is almost always called from a task spawned in
`onmounted`, and a task runs inside `render_immediate` while the document is
borrowed — an unconditional borrow panicked ("RefCell already borrowed")
before the first frame. A focus change asked for while the document is busy
is parked and applied the next time the document is free
(`apply_pending_focus`, from `poll` and `handle_ui_event`).
`get_scroll_offset` / `get_scroll_size` report a busy document or a dropped
node as an error instead of panicking.

Additionally (signal): `layout_stale` — a pointer event that arrives between
a mutation batch and the next frame's resolve walked layout links into freed
anonymous boxes and panicked; `handle_ui_event` resolves first when the tree
changed since.
