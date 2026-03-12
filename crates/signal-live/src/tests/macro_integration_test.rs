//! Comprehensive integration test for the macro system.
//!
//! Tests the full flow: block load → setup → registry → recording → parameter resolution

#[cfg(test)]
mod macro_integration {
    use signal_live::{
        macro_registry, macro_recorder::MacroRecorder, macro_setup::*, macro_templates,
    };
    use macromod::MacroBank;

    /// Scenario: Load block with 3-band EQ, drive all 3 parameters from one macro
    #[test]
    fn test_full_macro_workflow() {
        // 1. Setup: Create a mock block with macro bank
        let eq_bank = macro_templates::eq_3band();
        assert_eq!(eq_bank.knobs.len(), 3);

        // 2. Validate: Check that macro bank is valid
        signal_live::macro_error::validate_macro_bank(&eq_bank)
            .expect("EQ bank should be valid");

        // 3. Register: Simulate what setup_macros_for_block does
        let setup_result = MacroSetupResult {
            track_guid: "test-track-eq".to_string(),
            target_fx_guid: "test-fx-eq".to_string(),
            bindings: vec![
                LiveMacroBinding {
                    knob_index: 0,
                    knob_id: "eq_low".to_string(),
                    param_index: 1,
                    min: 0.0,
                    max: 1.0,
                },
                LiveMacroBinding {
                    knob_index: 1,
                    knob_id: "eq_mid".to_string(),
                    param_index: 5,
                    min: 0.0,
                    max: 1.0,
                },
                LiveMacroBinding {
                    knob_index: 2,
                    knob_id: "eq_high".to_string(),
                    param_index: 10,
                    min: 0.0,
                    max: 1.0,
                },
            ],
        };

        macro_registry::clear();
        macro_registry::register(&setup_result);

        // 4. Verify: Check that bindings were registered
        assert_eq!(macro_registry::knob_count(), 3);
        assert!(!macro_registry::is_empty());

        let (knobs, targets, avg) = macro_registry::stats();
        assert_eq!(knobs, 3);
        assert_eq!(targets, 3); // 1 target per knob in this case
        assert!((avg - 1.0).abs() < 0.01);

        // 5. Lookup: Get targets for each knob
        let eq_low_targets = macro_registry::get_targets("eq_low");
        assert_eq!(eq_low_targets.len(), 1);
        assert_eq!(eq_low_targets[0].param_index, 1);
        assert_eq!(eq_low_targets[0].min, 0.0);
        assert_eq!(eq_low_targets[0].max, 1.0);

        // 6. Record: Capture macro movements
        let recorder = MacroRecorder::new();
        recorder.start();

        // Simulate knob movements
        recorder.record("eq_low".into(), 0.5);
        recorder.record("eq_mid".into(), 0.7);
        recorder.record("eq_high".into(), 0.3);
        recorder.record("eq_low".into(), 0.6);

        let (count, duration, knobs) = recorder.stats();
        assert_eq!(count, 4);
        assert!(duration >= 0); // May be 0 in fast tests
        assert_eq!(knobs.len(), 3);
        assert!(knobs.contains(&"eq_low".to_string()));

        let recording = recorder.stop();
        assert_eq!(recording.len(), 4);
        assert_eq!(recording[0].knob_id, "eq_low");
        assert_eq!(recording[0].value, 0.5);
        assert_eq!(recording[3].knob_id, "eq_low");
        assert_eq!(recording[3].value, 0.6);

        // 7. Cleanup
        macro_registry::clear();
        assert!(macro_registry::is_empty());
    }

    /// Scenario: Multi-plugin macro binding (one macro controls multiple FX)
    #[test]
    fn test_multi_plugin_macro_routing() {
        macro_registry::clear();

        // Register EQ on track 1
        let eq_result = MacroSetupResult {
            track_guid: "track-1".to_string(),
            target_fx_guid: "eq-fx".to_string(),
            bindings: vec![LiveMacroBinding {
                knob_index: 0,
                knob_id: "drive".to_string(),
                param_index: 1,
                min: 0.0,
                max: 1.0,
            }],
        };
        macro_registry::register(&eq_result);

        // Register Compressor on track 1 (same macro, different param)
        let comp_result = MacroSetupResult {
            track_guid: "track-1".to_string(),
            target_fx_guid: "comp-fx".to_string(),
            bindings: vec![LiveMacroBinding {
                knob_index: 0,
                knob_id: "drive".to_string(),
                param_index: 3,
                min: 0.2,
                max: 0.8,
            }],
        };
        macro_registry::register(&comp_result);

        // Register Gate on track 2 (same macro name, different track)
        let gate_result = MacroSetupResult {
            track_guid: "track-2".to_string(),
            target_fx_guid: "gate-fx".to_string(),
            bindings: vec![LiveMacroBinding {
                knob_index: 0,
                knob_id: "drive".to_string(),
                param_index: 5,
                min: 0.0,
                max: 1.0,
            }],
        };
        macro_registry::register(&gate_result);

        // Verify: Single macro drives 3 FX parameters
        let targets = macro_registry::get_targets("drive");
        assert_eq!(targets.len(), 3);

        // Check each target
        assert_eq!(targets[0].param_index, 1);
        assert_eq!(targets[0].min, 0.0);

        assert_eq!(targets[1].param_index, 3);
        assert_eq!(targets[1].min, 0.2); // Different curve!

        assert_eq!(targets[2].param_index, 5);
        assert_eq!(targets[2].track_guid, "track-2");

        // Verify stats show merged targets
        let (knobs, target_count, _) = macro_registry::stats();
        assert_eq!(knobs, 1); // Only "drive" knob
        assert_eq!(target_count, 3); // 3 targets for that knob

        macro_registry::clear();
    }

    /// Scenario: Patch change clears stale bindings
    #[test]
    fn test_patch_change_lifecycle() {
        macro_registry::clear();

        // Load patch 1
        let patch1 = MacroSetupResult {
            track_guid: "track".to_string(),
            target_fx_guid: "fx1".to_string(),
            bindings: vec![LiveMacroBinding {
                knob_index: 0,
                knob_id: "drive".to_string(),
                param_index: 1,
                min: 0.0,
                max: 1.0,
            }],
        };
        macro_registry::register(&patch1);
        assert_eq!(macro_registry::knob_count(), 1);

        // Switch patch: clear old bindings
        macro_registry::clear();
        assert!(macro_registry::is_empty());

        // Load patch 2
        let patch2 = MacroSetupResult {
            track_guid: "track".to_string(),
            target_fx_guid: "fx2".to_string(),
            bindings: vec![
                LiveMacroBinding {
                    knob_index: 0,
                    knob_id: "tone".to_string(),
                    param_index: 5,
                    min: 0.0,
                    max: 1.0,
                },
                LiveMacroBinding {
                    knob_index: 1,
                    knob_id: "level".to_string(),
                    param_index: 10,
                    min: 0.0,
                    max: 1.0,
                },
            ],
        };
        macro_registry::register(&patch2);
        assert_eq!(macro_registry::knob_count(), 2);

        // Verify old "drive" binding is gone
        assert!(macro_registry::get_targets("drive").is_empty());

        // Verify new bindings exist
        assert!(!macro_registry::get_targets("tone").is_empty());
        assert!(!macro_registry::get_targets("level").is_empty());

        macro_registry::clear();
    }

    /// Scenario: Error handling for invalid configurations
    #[test]
    fn test_error_handling_invalid_inputs() {
        use signal_live::macro_error::{validate_macro_bank, MacroError};

        let empty_bank = MacroBank::default();
        let result = validate_macro_bank(&empty_bank);
        assert!(matches!(result, Err(MacroError::NoBindings)));
    }

    /// Scenario: Template configurations work correctly
    #[test]
    fn test_template_configurations() {
        let templates = vec![
            ("3-Band EQ", macro_templates::eq_3band()),
            ("Compressor", macro_templates::compressor()),
            ("Reverb", macro_templates::reverb()),
            ("Master Level", macro_templates::master_level()),
        ];

        for (name, bank) in templates {
            // Validate each template
            signal_live::macro_error::validate_macro_bank(&bank)
                .expect(&format!("{} should be valid", name));

            // Check knobs have bindings
            for knob in &bank.knobs {
                assert!(
                    !knob.bindings.is_empty(),
                    "{} knob {} should have bindings",
                    name,
                    knob.id
                );
            }
        }
    }
}
