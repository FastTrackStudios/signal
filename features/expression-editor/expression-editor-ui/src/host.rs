//! Dioxus adapter for the shared drum application services.
//! No file loading, window creation, or backend-specific saving belongs here.
use crate::quantize_panel::{Bin, HitPreview, QuantizePanel};
use dioxus::prelude::*;
use expression_editor_core::{Editor, drum::HitGesture, fills::FillConfig};
use expression_editor_host::{DrumDaw, SharedDrumHost};

#[derive(Default)]
pub struct HostCallbacks {
    pub error: Option<Signal<Option<String>>>,
    pub on_change: Option<EventHandler<QuantizePanel>>,
    pub on_apply: Option<EventHandler<QuantizePanel>>,
    pub on_save: Option<EventHandler<()>>,
    pub on_hit: Option<EventHandler<HitGesture>>,
    pub on_undo: Option<EventHandler<()>>,
    pub on_redo: Option<EventHandler<()>>,
}

/// Call unconditionally from a component body. Local and embedded hosts use
/// the same adapter; the application supplies any save action separately.
pub fn use_drum_callbacks<D: DrumDaw + 'static>(
    mut editor: Signal<Editor>,
    host: Option<SharedDrumHost<D>>,
    mut bins: Signal<Vec<Bin>>,
    mut previews: Signal<Vec<HitPreview>>,
    mut fills: Signal<Vec<(f64, f64)>>,
) -> HostCallbacks {
    let mut error = use_signal(|| None::<String>);
    let mut panel = use_signal(QuantizePanel::default);
    let host_identity = host.as_ref().map(std::sync::Arc::as_ptr);
    let initial_host = host.clone();
    use_effect(use_reactive!(|host_identity| {
        let _ = host_identity;
        if let Some(host) = &initial_host {
            fills.set(fill_spans(host));
        } else {
            fills.set(Vec::new());
        }
        bins.set(Vec::new());
        previews.set(Vec::new());
        error.set(None);
    }));
    let Some(host) = host else {
        return HostCallbacks::default();
    };

    let refresh = {
        let host = host.clone();
        move || {
            let (b, p) = host.preview(&panel.peek());
            bins.set(b);
            previews.set(p);
            fills.set(fill_spans(&host));
        }
    };
    let on_change = {
        let host = host.clone();
        EventHandler::new(move |settings: QuantizePanel| {
            let (b, p) = host.preview(&settings);
            panel.set(settings);
            bins.set(b);
            previews.set(p);
        })
    };
    let on_apply = {
        let host = host.clone();
        let mut refresh = refresh.clone();
        EventHandler::new(move |settings: QuantizePanel| match host.apply(&settings) {
            Ok(_) => {
                error.set(None);
                panel.set(settings);
                host.refresh_editor(&mut editor.write());
                refresh();
            }
            Err(failure) => {
                tracing::warn!(?failure, "quantize refused");
                error.set(Some(failure.to_string()));
            }
        })
    };
    let on_undo = {
        let host = host.clone();
        let mut refresh = refresh.clone();
        EventHandler::new(move |()| {
            if host.undo() {
                error.set(None);
                host.refresh_editor(&mut editor.write());
                refresh();
            }
        })
    };
    let on_redo = {
        let host = host.clone();
        let mut refresh = refresh.clone();
        EventHandler::new(move |()| {
            if host.redo() {
                error.set(None);
                host.refresh_editor(&mut editor.write());
                refresh();
            }
        })
    };
    let on_hit = {
        let mut refresh = refresh;
        EventHandler::new(
            move |gesture| match host.edit_hit(&mut editor.write(), gesture) {
                Ok(()) => {
                    error.set(None);
                    refresh();
                }
                Err(failure) => {
                    tracing::warn!(?failure, "drum edit refused");
                    error.set(Some(failure.to_string()));
                }
            },
        )
    };
    HostCallbacks {
        error: Some(error),
        on_change: Some(on_change),
        on_apply: Some(on_apply),
        on_hit: Some(on_hit),
        on_undo: Some(on_undo),
        on_redo: Some(on_redo),
        on_save: None,
    }
}

fn fill_spans<D: DrumDaw>(host: &SharedDrumHost<D>) -> Vec<(f64, f64)> {
    host.fills(&FillConfig::default())
        .iter()
        .map(|f| (f.start, f.end))
        .collect()
}
