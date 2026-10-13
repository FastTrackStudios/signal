//! Keyflow project scaffolding — REAPER-facing action contract.
//!
//! The trait only; `session::keyflow::scaffold::KeyflowScaffoldImpl<D>`
//! is the implementation. Same split as the other action contracts in
//! this crate.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait KeyflowScaffoldActions {
    #[action(
        description = "Scaffold a project from keyflow text: prompt for a chart, then build the Keyflow folder (KEY/CHORD/MELODY/SCALE tracks) and lay out one coloured region per song section.",
        category = "Keyflow",
        group = "Scaffold"
    )]
    fn scaffold_keyflow(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_matches_reaper_command_convention() {
        let ids: Vec<_> = KeyflowScaffoldActionsActions::all()
            .iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec!["FTS_SESSION_SCAFFOLD_KEYFLOW"]);
    }
}
