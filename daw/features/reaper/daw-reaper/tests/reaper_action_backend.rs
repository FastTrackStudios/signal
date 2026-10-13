//! Integration tests for `architect::action::ActionBackend for crate::Reaper`.
//!
//! Distinct from `reaper_action_registry.rs`, which exercises the
//! `ActionRegistration` RPC surface (list/lookup/execute REAPER's existing
//! actions). This file proves the *new* registration seam:
//! `#[architect::actions]`-declared traits register through
//! `architect::action::ActionBackend`, and the handler bound at
//! registration time is what REAPER actually invokes when the action fires
//! — no broadcast hop, no downstream string-match dispatch.
//!
//! Run with:
//!
//!   cargo xtask reaper-test -- reaper_action_backend

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use daw::test::reaper_test;

#[architect::actions(namespace = "FTS_TEST_ACTION_BACKEND")]
trait TestActions {
    #[action(
        description = "Increment a test counter",
        category = "Test",
        group = "Backend"
    )]
    fn bump(&self);
}

struct Counter(Arc<AtomicUsize>);

impl TestActions for Counter {
    fn bump(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[reaper_test(isolated)]
async fn action_backend_handler_runs_on_trigger(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let count = Arc::new(AtomicUsize::new(0));
    register_test_actions(&daw_reaper::Reaper, Arc::new(Counter(count.clone())));

    assert_eq!(
        TestActionsActions::all().len(),
        1,
        "one #[action]-decorated method should produce one ActionMeta"
    );
    assert_eq!(
        TestActionsActions::all()[0].id,
        "FTS_TEST_ACTION_BACKEND_BUMP"
    );

    let actions = ctx.daw.action_registry();
    let executed = actions
        .execute_named_action("FTS_TEST_ACTION_BACKEND_BUMP")
        .await?;
    assert!(
        executed,
        "execute_named_action should report success for a backend-registered action"
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "the ActionBackend-registered handler must run when REAPER triggers the action, \
         not just get its command id allocated"
    );

    Ok(())
}
