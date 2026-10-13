//! Validation of the interaction kinds and rules against a catalog.

use frame::surface::proto::*;

fn catalog() -> Catalog {
    Catalog {
        id: "t".into(),
        version: 1,
        params: vec![
            ParamInfo::new("eq/b1_gain", "Gain", Range::linear(-30.0, 30.0, Unit::Db)),
            ParamInfo::new("eq/b1_name", "Name", Range::default()).text(),
            ParamInfo::new("eq/b1_used", "Used", Range::default()).toggle(),
            ParamInfo::new("eq/range", "Range", Range::default()).choice(&["±6", "±12"]),
        ],
        ..Catalog::default()
    }
}

#[test]
fn text_fields_bind_text_params() {
    let ok = Control::new(ControlKind::TextField {
        click_to_edit: true,
        max_len: 0,
    })
    .bound(Binding::param("eq/b1_name"));
    assert!(ok.validate(&catalog()).is_empty());
    let bad = Control::new(ControlKind::TextField {
        click_to_edit: true,
        max_len: 0,
    })
    .bound(Binding::param("eq/b1_gain"));
    assert!(matches!(
        bad.validate(&catalog())[..],
        [BindingIssue::Mismatch { .. }]
    ));
}

#[test]
fn templates_validate_as_their_first_item() {
    let c = Control::new(ControlKind::Repeat {
        first: 1,
        count: 24,
        used: Some(ParamAddr::new("eq/b{n}_used")),
    });
    assert!(c.is_templated());
    assert!(c.validate(&catalog()).is_empty());
    let typo = Control::new(ControlKind::Repeat {
        first: 1,
        count: 24,
        used: Some(ParamAddr::new("eq/b{n}_usd")),
    });
    assert_eq!(
        typo.validate(&catalog()),
        [BindingIssue::UnknownParam {
            addr: ParamAddr::new("eq/b1_usd")
        }]
    );
    assert_eq!(
        c.instantiate(7, |s| s).extra_params(),
        [&ParamAddr::new("eq/b7_used")]
    );
}

#[test]
fn rules_commands_and_zoom_name_real_params() {
    let interact = Interact {
        click: vec![ClickRule {
            chord: Chord::OPTION,
            action: ClickAction::Run {
                commands: vec![Command::Toggle {
                    addr: ParamAddr::new("eq/b1_on"),
                }],
            },
        }],
        ..Interact::default()
    };
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Cycle,
    })
    .bound(Binding::param("eq/range"))
    .interacting(interact);
    assert_eq!(
        c.validate(&catalog()),
        [BindingIssue::UnknownParam {
            addr: ParamAddr::new("eq/b1_on")
        }]
    );
    c.interact = None;
    c.zoom = Some(Zoom {
        param: ParamAddr::new("eq/b1_gain"),
        spans: vec![6.0],
    });
    assert!(
        matches!(c.validate(&catalog())[..], [BindingIssue::Mismatch { .. }]),
        "a zoom follows a choice"
    );
    // A button that only runs commands needs no binding.
    let del = Control::new(ControlKind::Button {
        mode: ButtonMode::Trigger,
    })
    .interacting(Interact {
        press: vec![Command::Remove],
        ..Interact::default()
    });
    assert!(del.validate(&catalog()).is_empty());
}

#[test]
fn interaction_survives_json() {
    let c = Control::new(ControlKind::Popover {
        anchor: None,
        placement: Placement::Below,
        gap: 2.0,
        show: Show::Active,
        context: None,
    })
    .shown_if(Condition::any_of("eq/b1_shape", &[3.0, 4.0]).negated());
    let json = serde_json::to_string(&c).unwrap();
    assert_eq!(serde_json::from_str::<Control>(&json).unwrap(), c);
    // Older documents (no interaction fields) still load.
    let old = r#"{"kind":"Readout","binding":null,"y_binding":null,"midi":null,"default":null,"slots":[]}"#;
    assert!(
        serde_json::from_str::<Control>(old)
            .unwrap()
            .interact
            .is_none()
    );
}
