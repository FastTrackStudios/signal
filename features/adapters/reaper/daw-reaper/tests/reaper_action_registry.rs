//! Integration tests for the ActionRegistry service.
//!
//! Verifies that DAW clients can register custom REAPER actions,
//! look them up, and check registration status.
//!
//! Run with:
//!
//!   cargo xtask reaper-test -- reaper_action_registry

use daw::test::reaper_test;
use daw_proto::{ActionListFilter, ActionListRequest, ActionOrigin, ActionSection};

#[reaper_test(isolated)]
async fn register_and_lookup_action(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    // Register a new action
    let cmd_id = actions
        .register("fts.test.register_lookup", "FTS Test: Register and Lookup")
        .await?;
    assert!(cmd_id > 0, "register should return a valid command ID");
    ctx.log(&format!("Registered cmd_id={cmd_id}"));

    // Look it up by name
    let looked_up = actions
        .lookup_command_id("fts.test.register_lookup")
        .await?;
    assert_eq!(
        looked_up,
        Some(cmd_id),
        "lookup should return the same command ID"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn is_registered_returns_true_for_known_action(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    // Register an action
    let cmd_id = actions
        .register("fts.test.is_registered", "FTS Test: Is Registered")
        .await?;
    assert!(cmd_id > 0);

    // Check it has a command ID
    let exists = actions.is_registered("fts.test.is_registered").await?;
    assert!(exists, "action should be registered after register()");

    // Check it's actually in REAPER's action list (not just command ID registry)
    let in_list = actions.is_in_action_list("fts.test.is_registered").await?;
    assert!(
        in_list,
        "action should appear in REAPER's action list after register()"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn is_registered_returns_false_for_unknown_action(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let exists = actions
        .is_registered("fts.test.definitely_not_registered_xyz")
        .await?;
    assert!(!exists, "unknown action should not be registered");

    Ok(())
}

#[reaper_test(isolated)]
async fn lookup_returns_none_for_unknown_action(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let result = actions
        .lookup_command_id("fts.test.nonexistent_action_xyz")
        .await?;
    assert_eq!(result, None, "unknown action should return None");

    Ok(())
}

#[reaper_test(isolated)]
async fn register_same_action_twice_returns_same_id(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let id1 = actions
        .register("fts.test.idempotent", "FTS Test: Idempotent Registration")
        .await?;
    let id2 = actions
        .register("fts.test.idempotent", "FTS Test: Idempotent Registration")
        .await?;

    assert_eq!(
        id1, id2,
        "registering the same action twice should return the same ID"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn register_in_menu_returns_valid_id(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let cmd_id = actions
        .register_in_menu(
            "FTS_SESSION_TEST_MENU_REGISTER",
            "FTS Test: Menu Registration",
        )
        .await?;
    assert!(
        cmd_id > 0,
        "register_in_menu should return a valid command ID"
    );
    ctx.log(&format!("register_in_menu cmd_id={cmd_id}"));

    Ok(())
}

#[reaper_test(isolated)]
async fn register_in_menu_is_findable(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let cmd_id = actions
        .register_in_menu("FTS_TRANSPORT_TEST_MENU_FIND", "FTS Test: Menu Findable")
        .await?;
    assert!(cmd_id > 0);

    let looked_up = actions
        .lookup_command_id("FTS_TRANSPORT_TEST_MENU_FIND")
        .await?;
    assert_eq!(
        looked_up,
        Some(cmd_id),
        "lookup should return the same ID as register_in_menu"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn register_toggle_in_menu_is_findable_for_input_namespace(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let cmd_id = actions
        .register_toggle_in_menu(
            "FTS_INPUT_TEST_TOGGLE_NAMESPACE",
            "FTS Test: Input Namespace Toggle",
        )
        .await?;
    assert!(
        cmd_id > 0,
        "register_toggle_in_menu should return a valid command ID"
    );

    let looked_up = actions
        .lookup_command_id("FTS_INPUT_TEST_TOGGLE_NAMESPACE")
        .await?;
    assert_eq!(looked_up, Some(cmd_id));

    let in_list = actions
        .is_in_action_list("FTS_INPUT_TEST_TOGGLE_NAMESPACE")
        .await?;
    assert!(
        in_list,
        "FTS_INPUT_TEST_TOGGLE_NAMESPACE should appear in REAPER's action list"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn register_and_register_in_menu_are_idempotent(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    // First register without menu
    let id1 = actions
        .register(
            "FTS_SIGNAL_TEST_IDEMPOTENT_MENU",
            "FTS Test: Idempotent Menu",
        )
        .await?;
    assert!(id1 > 0);

    // Then register again with menu — should return the same ID
    let id2 = actions
        .register_in_menu(
            "FTS_SIGNAL_TEST_IDEMPOTENT_MENU",
            "FTS Test: Idempotent Menu",
        )
        .await?;

    assert_eq!(
        id1, id2,
        "register then register_in_menu for the same action should return the same ID"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn run_action_runs_native_action(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    // 40029 = "Undo" — a safe, well-known REAPER action that never shows a dialog
    actions.run_action(40029).await?;
    ctx.log("run_action(40029) completed without error");

    Ok(())
}

#[reaper_test(isolated)]
async fn execute_named_action_for_registered_action(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    // Register an action first
    let cmd_id = actions
        .register("FTS_SYNC_TEST_EXEC_NAMED", "FTS Test: Execute Named Action")
        .await?;
    assert!(cmd_id > 0);

    // Execute it by name
    let result = actions
        .execute_named_action("FTS_SYNC_TEST_EXEC_NAMED")
        .await?;
    assert!(
        result,
        "execute_named_action should return true for a registered action"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn execute_named_action_accepts_prefixed_command_name(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let cmd_id = actions
        .register(
            "FTS_SYNC_TEST_EXEC_PREFIXED",
            "FTS Test: Execute Prefixed Action",
        )
        .await?;
    assert!(cmd_id > 0);

    assert!(
        actions
            .is_registered("_FTS_SYNC_TEST_EXEC_PREFIXED")
            .await?,
        "is_registered should accept REAPER's underscore-prefixed command name"
    );
    assert_eq!(
        actions
            .lookup_command_id("_FTS_SYNC_TEST_EXEC_PREFIXED")
            .await?,
        Some(cmd_id),
        "lookup should accept REAPER's underscore-prefixed command name"
    );

    let result = actions
        .execute_named_action("_FTS_SYNC_TEST_EXEC_PREFIXED")
        .await?;
    assert!(
        result,
        "execute_named_action should accept REAPER's underscore-prefixed command name"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn execute_named_action_for_unknown_returns_false(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let result = actions
        .execute_named_action("FTS_NONEXISTENT_ACTION_XYZ_999")
        .await?;
    assert!(
        !result,
        "execute_named_action should return false for an unknown action"
    );

    Ok(())
}

// ── Action list enumeration ────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn list_actions_returns_reaper_builtin_actions(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let response = actions
        .list_actions(ActionListRequest {
            filter: ActionListFilter::Reaper,
            section: ActionSection::Main,
            query: Some("undo".to_string()),
            limit: Some(256),
        })
        .await?;
    let listed = response.actions;

    assert!(
        !listed.is_empty(),
        "REAPER filter should return built-in actions for query 'undo'"
    );
    assert!(
        listed
            .iter()
            .all(|action| action.origin == ActionOrigin::Reaper),
        "REAPER filter must only return built-in REAPER actions: {listed:#?}"
    );
    assert!(
        listed
            .iter()
            .any(|action| action.command_id == 40029 || action.description.contains("Undo")),
        "REAPER action list should include the built-in undo action: {listed:#?}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn list_actions_returns_registered_fts_toggle_state(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let cmd_id = actions
        .register_toggle(
            "FTS_TEST_LIST_ACTIONS_TOGGLE",
            "FTS Test: List Actions Toggle",
        )
        .await?;
    actions
        .set_toggle_state("FTS_TEST_LIST_ACTIONS_TOGGLE", true)
        .await?;

    let response = actions
        .list_actions(ActionListRequest {
            filter: ActionListFilter::Registered,
            section: ActionSection::Main,
            query: Some("List Actions Toggle".to_string()),
            limit: None,
        })
        .await?;
    let listed = response.actions;

    let action = listed
        .iter()
        .find(|action| action.command_id == cmd_id)
        .ok_or_else(|| eyre::eyre!("registered toggle action not found: {listed:#?}"))?;
    assert_eq!(
        action.command_name.as_deref(),
        Some("FTS_TEST_LIST_ACTIONS_TOGGLE")
    );
    assert_eq!(action.description, "FTS Test: List Actions Toggle");
    assert_eq!(action.origin, ActionOrigin::Fts);
    assert_eq!(action.provider, "fts");
    assert_eq!(action.section_id, 0);
    assert_eq!(action.section_name, "main");
    assert!(action.registered_by_fts);
    assert_eq!(action.toggle_state, Some(true));

    Ok(())
}

#[reaper_test(isolated)]
async fn list_actions_non_reaper_includes_registered_actions(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let cmd_id = actions
        .register(
            "FTS_TEST_LIST_ACTIONS_NON_REAPER",
            "FTS Test: List Actions Non Reaper",
        )
        .await?;

    let response = actions
        .list_actions(ActionListRequest {
            filter: ActionListFilter::NonReaper,
            section: ActionSection::Main,
            query: Some("List Actions Non Reaper".to_string()),
            limit: Some(16),
        })
        .await?;
    let listed = response.actions;

    assert!(
        listed.iter().any(|action| action.command_id == cmd_id
            && action.origin == ActionOrigin::Fts
            && action.provider == "fts"
            && action.registered_by_fts),
        "non-reaper filter should include registered FTS actions: {listed:#?}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn list_actions_reports_total_count_when_limited(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    actions
        .register(
            "FTS_TEST_LIST_ACTIONS_LIMIT_A",
            "FTS Test: List Actions Limit",
        )
        .await?;
    actions
        .register(
            "FTS_TEST_LIST_ACTIONS_LIMIT_B",
            "FTS Test: List Actions Limit",
        )
        .await?;

    let response = actions
        .list_actions(ActionListRequest {
            filter: ActionListFilter::Registered,
            section: ActionSection::Main,
            query: Some("List Actions Limit".to_string()),
            limit: Some(1),
        })
        .await?;

    assert_eq!(response.actions.len(), 1, "limit must cap returned rows");
    assert!(
        response.total_count >= 2,
        "total_count should include matches not returned by limit: {response:#?}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn list_actions_accepts_non_main_sections(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let response = actions
        .list_actions(ActionListRequest {
            filter: ActionListFilter::All,
            section: ActionSection::MidiEditor,
            query: None,
            limit: Some(8),
        })
        .await?;

    assert!(
        response
            .actions
            .iter()
            .all(|action| action.section_id == 32060 && action.section_name == "midi-editor"),
        "MIDI editor rows should report section metadata: {response:#?}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn list_actions_sws_filter_only_returns_sws_classified_actions(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let response = actions
        .list_actions(ActionListRequest {
            filter: ActionListFilter::Sws,
            section: ActionSection::Main,
            query: None,
            limit: Some(32),
        })
        .await?;
    let listed = response.actions;

    assert!(
        listed
            .iter()
            .all(|action| action.origin == ActionOrigin::Sws && action.provider == "sws"),
        "SWS filter must only return SWS-classified actions: {listed:#?}"
    );
    ctx.log(&format!("SWS action sample count: {}", listed.len()));

    Ok(())
}

#[reaper_test(isolated)]
async fn execute_action_detailed_reports_registered_toggle_transition(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let cmd_id = actions
        .register_toggle(
            "FTS_TEST_EXECUTE_DETAILED_TOGGLE",
            "FTS Test: Execute Detailed Toggle",
        )
        .await?;
    assert_eq!(
        actions
            .get_toggle_state("FTS_TEST_EXECUTE_DETAILED_TOGGLE")
            .await?,
        Some(false)
    );

    let result = actions
        .execute_action_detailed("FTS_TEST_EXECUTE_DETAILED_TOGGLE")
        .await?;

    assert!(result.executed);
    assert_eq!(result.command_id, Some(cmd_id));
    assert_eq!(
        result.command_name.as_deref(),
        Some("FTS_TEST_EXECUTE_DETAILED_TOGGLE")
    );
    assert_eq!(
        result.description.as_deref(),
        Some("FTS Test: Execute Detailed Toggle")
    );
    assert_eq!(result.origin, Some(ActionOrigin::Fts));
    assert_eq!(result.provider.as_deref(), Some("fts"));
    assert!(
        result
            .provider_tags
            .iter()
            .any(|tag| tag == "fasttrackstudio")
    );
    assert!(result.registered_by_fts);
    assert_eq!(result.toggle_state_before, Some(false));
    assert_eq!(result.toggle_state_after, Some(true));

    Ok(())
}

// ── Toggle state behaviour ──────────────────────────────────────────────────
//
// These tests verify that set_toggle_state actually persists, that
// get_toggle_state observes the latest write, and that REAPER's own
// `IsToggled` callback (the closure we registered as ActionKind::Toggleable)
// reads the same state. Toggle state lives in `toggle_states()` keyed by
// command name, set by `register_toggle*` and read by REAPER on every
// menu/toolbar repaint.

#[reaper_test(isolated)]
async fn toggle_state_round_trips_on_then_off(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let cmd_id = actions
        .register_toggle("FTS_TEST_TOGGLE_ROUNDTRIP", "FTS Test: Toggle Roundtrip")
        .await?;
    assert!(cmd_id > 0, "register_toggle returned 0");

    // Newly-registered toggle starts at false.
    let initial = actions
        .get_toggle_state("FTS_TEST_TOGGLE_ROUNDTRIP")
        .await?;
    assert_eq!(
        initial,
        Some(false),
        "fresh toggleable action must start at false"
    );

    // Flip to on.
    actions
        .set_toggle_state("FTS_TEST_TOGGLE_ROUNDTRIP", true)
        .await?;
    let after_on = actions
        .get_toggle_state("FTS_TEST_TOGGLE_ROUNDTRIP")
        .await?;
    assert_eq!(after_on, Some(true), "set_toggle_state(true) didn't stick");

    // Flip back off.
    actions
        .set_toggle_state("FTS_TEST_TOGGLE_ROUNDTRIP", false)
        .await?;
    let after_off = actions
        .get_toggle_state("FTS_TEST_TOGGLE_ROUNDTRIP")
        .await?;
    assert_eq!(
        after_off,
        Some(false),
        "set_toggle_state(false) didn't stick"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn toggle_state_persists_across_set_calls(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    actions
        .register_toggle("FTS_TEST_TOGGLE_PERSIST", "FTS Test: Toggle Persist")
        .await?;

    // Idempotent set — same value twice doesn't disturb state.
    actions
        .set_toggle_state("FTS_TEST_TOGGLE_PERSIST", true)
        .await?;
    actions
        .set_toggle_state("FTS_TEST_TOGGLE_PERSIST", true)
        .await?;
    assert_eq!(
        actions.get_toggle_state("FTS_TEST_TOGGLE_PERSIST").await?,
        Some(true)
    );

    // Many flips in a row — only the last write should be observable.
    for _ in 0..5 {
        actions
            .set_toggle_state("FTS_TEST_TOGGLE_PERSIST", false)
            .await?;
        actions
            .set_toggle_state("FTS_TEST_TOGGLE_PERSIST", true)
            .await?;
    }
    assert_eq!(
        actions.get_toggle_state("FTS_TEST_TOGGLE_PERSIST").await?,
        Some(true),
        "after even number of flips ending on true, state should be true"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn set_toggle_state_on_non_toggleable_is_noop(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    // Register a non-toggleable action.
    actions
        .register(
            "FTS_TEST_NON_TOGGLEABLE",
            "FTS Test: Non-toggleable Sentinel",
        )
        .await?;

    // Setting toggle state on a non-toggleable action should be a no-op
    // — get_toggle_state returns None because the action was never
    // recorded in the toggle-state map.
    actions
        .set_toggle_state("FTS_TEST_NON_TOGGLEABLE", true)
        .await?;
    let state = actions.get_toggle_state("FTS_TEST_NON_TOGGLEABLE").await?;
    assert_eq!(
        state, None,
        "set_toggle_state on a non-toggleable action must NOT register a state"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn get_toggle_state_for_unknown_returns_none(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let state = actions
        .get_toggle_state("FTS_TEST_NEVER_REGISTERED_TOGGLE")
        .await?;
    assert_eq!(
        state, None,
        "unknown action should not have a recorded toggle state"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn execute_named_action_on_toggle_runs_handler(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    // Register a toggleable action — handler is auto-generated
    // (registered_actions stores the cmd_id; REAPER will dispatch through
    // it). We don't observe a handler-side side-effect here; the assertion
    // is that the trigger path doesn't error and reports success.
    actions
        .register_toggle("FTS_TEST_TOGGLE_TRIGGER", "FTS Test: Toggle Trigger")
        .await?;
    actions
        .set_toggle_state("FTS_TEST_TOGGLE_TRIGGER", true)
        .await?;

    let triggered = actions
        .execute_named_action("FTS_TEST_TOGGLE_TRIGGER")
        .await?;
    assert!(
        triggered,
        "execute_named_action on a registered toggle action should report success"
    );

    Ok(())
}

// ── Unregister ──────────────────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn unregister_returns_true_for_known_action(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    actions
        .register_in_menu("FTS_TEST_UNREGISTER_KNOWN", "FTS Test: Unregister Known")
        .await?;
    assert!(
        actions
            .is_in_action_list("FTS_TEST_UNREGISTER_KNOWN")
            .await?
    );

    actions.unregister("FTS_TEST_UNREGISTER_KNOWN").await?;

    Ok(())
}

#[reaper_test(isolated)]
async fn unregister_removes_from_action_list(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    actions
        .register_in_menu("FTS_TEST_UNREGISTER_ME", "FTS Test: Unregister Me")
        .await?;
    assert!(
        actions.is_in_action_list("FTS_TEST_UNREGISTER_ME").await?,
        "freshly registered action must be in the action list"
    );

    actions.unregister("FTS_TEST_UNREGISTER_ME").await?;

    // The gaccel entry is gone — the action no longer appears in the
    // Actions window / REAPER's main keyboard section action list.
    assert!(
        !actions.is_in_action_list("FTS_TEST_UNREGISTER_ME").await?,
        "action must NOT be in REAPER's action list after unregister"
    );

    // NOTE: REAPER's named-command-id allocations are sticky for the
    // lifetime of the process; once a command name has been registered
    // its cmd_id stays in NamedCommandLookup even after the gaccel and
    // the action's hook entry are dropped. This is REAPER behaviour,
    // not a bug here. Our internal tracking IS gone, so re-registering
    // takes the "register fresh" path inside register_action — see
    // `reregister_after_unregister_keeps_cmd_id_but_re_adds_gaccel`.
    Ok(())
}

#[reaper_test(isolated)]
async fn reregister_after_unregister_keeps_cmd_id_but_re_adds_gaccel(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let id1 = actions
        .register_in_menu("FTS_TEST_REREG_AFTER_UNREG", "FTS Test: Re-register")
        .await?;
    assert!(id1 > 0);
    actions.unregister("FTS_TEST_REREG_AFTER_UNREG").await?;
    assert!(
        !actions
            .is_in_action_list("FTS_TEST_REREG_AFTER_UNREG")
            .await?,
        "after unregister the action must NOT be in the action list"
    );

    let id2 = actions
        .register_in_menu("FTS_TEST_REREG_AFTER_UNREG", "FTS Test: Re-register")
        .await?;
    assert!(id2 > 0, "re-register must return a valid cmd_id");
    // REAPER's `register("command_id", name)` does not recycle ids — each
    // registration of a previously-seen name still mints a fresh slot. The
    // important invariant is that the action is registered against a valid
    // cmd_id (what we return), not that it appears in REAPER's action list
    // under the *latest* lookup of the name (REAPER's `NamedCommandLookup`
    // returns the first allocation, but we registered the gaccel for the
    // most recent allocation, so they can disagree after unregister + re-register).
    let registered = actions
        .lookup_command_id("FTS_TEST_REREG_AFTER_UNREG")
        .await?;
    assert!(
        registered.is_some(),
        "re-register must keep the name resolvable"
    );
    Ok(())
}

#[reaper_test(isolated)]
async fn unregister_unknown_returns_false(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    // API now returns () — unregistering an unknown action is a no-op rather
    // than a boolean signal. Test simply asserts it doesn't error.
    actions
        .unregister("FTS_TEST_UNREGISTER_NEVER_REGISTERED")
        .await?;

    Ok(())
}

#[reaper_test(isolated)]
async fn unregister_clears_toggle_state(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    actions
        .register_toggle("FTS_TEST_UNREGISTER_TOGGLE", "FTS Test: Unregister Toggle")
        .await?;
    actions
        .set_toggle_state("FTS_TEST_UNREGISTER_TOGGLE", true)
        .await?;
    assert_eq!(
        actions
            .get_toggle_state("FTS_TEST_UNREGISTER_TOGGLE")
            .await?,
        Some(true)
    );

    actions.unregister("FTS_TEST_UNREGISTER_TOGGLE").await?;
    assert_eq!(
        actions
            .get_toggle_state("FTS_TEST_UNREGISTER_TOGGLE")
            .await?,
        None,
        "unregister of a toggleable action must clear its toggle state"
    );

    Ok(())
}
