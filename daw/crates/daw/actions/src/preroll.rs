use daw::service::preroll::{PreRollActions, register_pre_roll_actions};
use daw::service::{ActionRegistration, ProjectContext, Projects};

const PROJECT_PRE_ROLL_MEASURES_KEY: &str = "prerollmeas";
const EPSILON_MEASURES: f64 = 0.000_1;
const SET_HALF_MEASURE_COMMAND: &str = "FTS_SESSION_PRE_ROLL_SET_HALF_MEASURE";
const SET_1_MEASURE_COMMAND: &str = "FTS_SESSION_PRE_ROLL_SET_1_MEASURE";
const SET_2_MEASURES_COMMAND: &str = "FTS_SESSION_PRE_ROLL_SET_2_MEASURES";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreRollAction {
    Double,
    Half,
    SetMeasures(f64),
}

pub fn init<D>(daw: &D)
where
    D: Projects + ActionRegistration,
{
    if let Err(err) = update_toggle_states_from_project(daw) {
        tracing::warn!(
            ?err,
            "[session] Failed to initialize pre-roll toggle states"
        );
    }
}

pub fn dispatch<D>(daw: &D, action: PreRollAction)
where
    D: Projects + ActionRegistration,
{
    if let Err(err) = run_action(daw, action) {
        tracing::error!(?action, ?err, "[session] Pre-roll action failed");
    }
}

fn run_action<D>(daw: &D, action: PreRollAction) -> eyre::Result<()>
where
    D: Projects + ActionRegistration,
{
    let project = ProjectContext::Current;
    let current = current_pre_roll_measures(daw, project.clone());
    let next = match action {
        PreRollAction::Double => {
            if current > 0.0 {
                current * 2.0
            } else {
                1.0
            }
        }
        PreRollAction::Half => {
            if current > 0.0 {
                current / 2.0
            } else {
                0.5
            }
        }
        PreRollAction::SetMeasures(measures) => measures,
    };

    set_pre_roll_measures(daw, project, next)?;
    update_toggle_states(daw, next);
    Ok(())
}

fn set_pre_roll_measures<D>(daw: &D, project: ProjectContext, measures: f64) -> eyre::Result<()>
where
    D: Projects,
{
    let measures = measures.max(0.0);
    if !daw.set_project_config(project, PROJECT_PRE_ROLL_MEASURES_KEY, measures) {
        eyre::bail!("REAPER project config key {PROJECT_PRE_ROLL_MEASURES_KEY} is unavailable");
    }

    tracing::info!("[session] Set pre-roll duration to {measures} measures");
    Ok(())
}

fn update_toggle_states<D>(daw: &D, current_measures: f64)
where
    D: ActionRegistration,
{
    daw.set_toggle_state(
        SET_HALF_MEASURE_COMMAND,
        (current_measures - 0.5).abs() < EPSILON_MEASURES,
    );
    daw.set_toggle_state(
        SET_1_MEASURE_COMMAND,
        (current_measures - 1.0).abs() < EPSILON_MEASURES,
    );
    daw.set_toggle_state(
        SET_2_MEASURES_COMMAND,
        (current_measures - 2.0).abs() < EPSILON_MEASURES,
    );
}

fn update_toggle_states_from_project<D>(daw: &D) -> eyre::Result<()>
where
    D: Projects + ActionRegistration,
{
    let current = current_pre_roll_measures(daw, ProjectContext::Current);
    update_toggle_states(daw, current);
    Ok(())
}

fn current_pre_roll_measures<D>(daw: &D, project: ProjectContext) -> f64
where
    D: Projects,
{
    daw.get_project_config(project.clone(), PROJECT_PRE_ROLL_MEASURES_KEY)
        .unwrap_or_else(|| daw.get_project_info(project, PROJECT_PRE_ROLL_MEASURES_KEY))
}

// ── architect::actions implementation ───────────────────────────────────
//
// The action enum + `dispatch` below stay: they are the shared body every
// action method (and, where one exists, the RPC service impl) calls into.
// What's gone is the string-keyed `action_for_id` lookup and the
// `session_actions` `define_actions!` entries that declared the same
// `FTS_SESSION_*` command ids a second time.

/// Bridges the five pre-roll actions onto `#[architect::actions]`. Every
/// method forwards to the existing synchronous `dispatch` — no behavior
/// change, just a declarative front door with real metadata.
pub struct PreRollActionsImpl<D> {
    daw: D,
}

impl<D> PreRollActions for PreRollActionsImpl<D>
where
    D: Projects + ActionRegistration,
{
    fn pre_roll_double_duration(&self) {
        dispatch(&self.daw, PreRollAction::Double);
    }
    fn pre_roll_half_duration(&self) {
        dispatch(&self.daw, PreRollAction::Half);
    }
    fn pre_roll_set_half_measure(&self) {
        dispatch(&self.daw, PreRollAction::SetMeasures(0.5));
    }
    fn pre_roll_set_1_measure(&self) {
        dispatch(&self.daw, PreRollAction::SetMeasures(1.0));
    }
    fn pre_roll_set_2_measures(&self) {
        dispatch(&self.daw, PreRollAction::SetMeasures(2.0));
    }
}

/// Registers all five pre-roll actions with `backend`, dispatching each
/// through a fresh `PreRollActionsImpl` bound to `daw`.
pub fn register_actions<D, B>(backend: &B, daw: D)
where
    D: Projects + ActionRegistration + Send + Sync + 'static,
    B: ::architect::action::ActionBackend + ?Sized,
{
    register_pre_roll_actions(backend, std::sync::Arc::new(PreRollActionsImpl { daw }));
}
