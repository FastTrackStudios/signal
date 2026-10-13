//! Every built-in's state comes back as it went: saved, loaded into a
//! fresh instance and saved again, the same bytes — so a session saved
//! after a reopen is the session that was opened.

#[cfg(test)]
mod state_round_trip {
    use daw_standalone::plugin::FxFactory;

    #[test]
    fn every_builtin_saves_what_it_loaded() {
        let factory = builtin_fx::BuiltinFx;
        let names: Vec<String> = factory.installed().into_iter().map(|fx| fx.name).collect();
        assert!(names.len() > 5, "the built-ins: {names:?}");
        for name in names {
            let mut first = factory.create(&name, 48_000.0).expect("made");
            first.prepare(48_000.0, 512).expect("prepared");
            let saved = first.save_state().expect("saved");
            let mut second = factory.create(&name, 48_000.0).expect("made");
            second.load_state(&saved).expect("loaded");
            second.prepare(48_000.0, 512).expect("prepared");
            let again = second.save_state().expect("saved again");
            assert!(
                saved == again,
                "{name}: {} bytes, then {}",
                saved.len(),
                again.len()
            );
        }
    }
}
