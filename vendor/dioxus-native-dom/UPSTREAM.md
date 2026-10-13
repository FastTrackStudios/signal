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
