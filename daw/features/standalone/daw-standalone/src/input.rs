//! `impl Input for Standalone` — in-memory key filter, no real keyboard hook.

use daw_proto::{Input, KeyFilter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::sync::Standalone;

static ENABLED: AtomicBool = AtomicBool::new(false);
static FILTER: OnceLock<Mutex<KeyFilter>> = OnceLock::new();

fn filter() -> &'static Mutex<KeyFilter> {
    FILTER.get_or_init(|| Mutex::new(KeyFilter::PassAll))
}

impl Input for Standalone {
    fn set_key_filter(&self, f: KeyFilter) {
        *filter().lock().unwrap() = f;
    }

    fn get_key_filter(&self) -> KeyFilter {
        filter().lock().unwrap().clone()
    }

    fn set_enabled(&self, enabled: bool) {
        ENABLED.store(enabled, Ordering::Relaxed);
    }

    fn is_enabled(&self) -> bool {
        ENABLED.load(Ordering::Relaxed)
    }

    fn execute_action(&self, _action_id: &str) {}
}
