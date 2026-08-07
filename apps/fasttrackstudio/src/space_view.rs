//! Sample-space map view (#77 M2): pan-zoom scatter of a built `.space`,
//! class-colored, click-to-audition, similarity side list, class + text
//! filters that re-scope BOTH the map and the list (the XO rule — the
//! filter is sent to the engine, which applies it to map() and similar()).

use dioxus::prelude::*;
use signal_space_proto::space::SampleSpaceClient;
use signal_space_proto::{MapItem, SpaceFilter, SpaceInfo};

use crate::remote::{establish, EngineTarget};

const CLASS_COLORS: &[(&str, &str)] = &[
    ("kick", "#e05252"),
    ("snare", "#e0a852"),
    ("clap", "#d8e052"),
    ("hat-closed", "#52e07e"),
    ("hat-open", "#52e0d0"),
    ("cymbal", "#52a8e0"),
    ("tom", "#a852e0"),
    ("perc", "#e052c8"),
    ("fx", "#8a8fa3"),
    ("other", "#555a66"),
];

fn class_color(class: &str) -> &'static str {
    CLASS_COLORS
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, col)| *col)
        .unwrap_or("#8a8fa3")
}

#[component]
pub fn SpaceView() -> Element {
    let client = use_resource(move || async move {
        let target = EngineTarget::current();
        loop {
            let c: Option<SampleSpaceClient> = establish(&target).await;
            if let Some(c) = c {
                return c;
            }
            architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
        }
    });
    let mut spaces = use_signal(Vec::<SpaceInfo>::new);
    let mut active = use_signal(String::new);
    let mut text_filter = use_signal(String::new);
    let mut class_off = use_signal(Vec::<String>::new);
    let mut selected = use_signal(|| None::<MapItem>);
    let mut similar = use_signal(Vec::<signal_space_proto::SimilarHit>::new);
    let mut items = use_signal(Vec::<MapItem>::new);

    let filter = move || {
        let off = class_off();
        let classes = if off.is_empty() {
            Vec::new()
        } else {
            CLASS_COLORS
                .iter()
                .map(|(c, _)| c.to_string())
                .filter(|c| !off.contains(c))
                .collect()
        };
        SpaceFilter { classes, text: text_filter(), ..Default::default() }
    };

    // Space list on connect.
    use_effect(move || {
        if let Some(c) = client() {
            spawn(async move {
                let list = c.spaces().await.unwrap_or_default();
                if active.peek().is_empty()
                    && let Some(first) = list.first() {
                        active.set(first.name.clone());
                    }
                spaces.set(list);
            });
        }
    });
    // Map refetch on space/filter change.
    use_effect(move || {
        let name = active();
        let f = filter();
        if name.is_empty() {
            return;
        }
        if let Some(c) = client() {
            spawn(async move {
                items.set(c.map(name, f).await.unwrap_or_default());
            });
        }
    });

    let mut pick = move |it: MapItem| {
        let name = active.peek().clone();
        let f = filter();
        let idx = it.idx;
        selected.set(Some(it));
        if let Some(c) = client() {
            spawn(async move {
                let _ = c.audition(name.clone(), idx).await;
                similar.set(c.similar(name, idx, f).await.unwrap_or_default());
            });
        }
    };

    rsx! {
        div {
            style: "display:flex; flex-direction:row; gap:12px; padding:12px; height:100%; \
                    box-sizing:border-box; color:#e6e8ef; background:#14161c; min-height:0;",
            // ── map ──
            div {
                style: "flex:1; display:flex; flex-direction:column; gap:8px; min-width:0;",
                // toolbar: space picker + text filter + class chips
                div {
                    style: "display:flex; flex-direction:row; gap:8px; align-items:center; flex-wrap:wrap;",
                    for s in spaces() {
                        button {
                            style: format!(
                                "padding:4px 10px; border-radius:6px; border:1px solid #333a4a; \
                                 background:{}; color:#e6e8ef; cursor:pointer;",
                                if active() == s.name { "#2c3242" } else { "#1a1e28" }
                            ),
                            onclick: move |_| active.set(s.name.clone()),
                            "{s.name} ({s.item_count})"
                        }
                    }
                    input {
                        style: "margin-left:auto; padding:4px 8px; border-radius:6px; \
                                border:1px solid #333a4a; background:#1a1e28; color:#e6e8ef;",
                        placeholder: "filter…",
                        value: "{text_filter}",
                        oninput: move |e| text_filter.set(e.value()),
                    }
                }
                div {
                    style: "display:flex; flex-direction:row; gap:6px; flex-wrap:wrap;",
                    for (class, color) in CLASS_COLORS.iter().copied() {
                        button {
                            style: format!(
                                "padding:2px 8px; border-radius:10px; border:1px solid {color}; \
                                 background:{}; color:{}; cursor:pointer; font-size:11px;",
                                if class_off().contains(&class.to_string()) { "transparent" } else { color },
                                if class_off().contains(&class.to_string()) { color } else { "#14161c" },
                            ),
                            onclick: move |_| {
                                let mut off = class_off();
                                let c = class.to_string();
                                if let Some(p) = off.iter().position(|x| *x == c) {
                                    off.remove(p);
                                } else {
                                    off.push(c);
                                }
                                class_off.set(off);
                            },
                            "{class}"
                        }
                    }
                }
                // scatter
                div {
                    style: "position:relative; flex:1; min-height:0; background:#0e1015; \
                            border:1px solid #262b38; border-radius:8px; overflow:hidden;",
                    for it in items() {
                        {
                            let sel = selected().is_some_and(|s| s.idx == it.idx);
                            let color = class_color(&it.class);
                            let size = if sel { 14.0 } else { 9.0 };
                            let it2 = it.clone();
                            rsx! {
                                div {
                                    title: "{it.path}",
                                    style: format!(
                                        "position:absolute; left:calc({}% - {size}px/2); \
                                         top:calc({}% - {size}px/2); width:{size}px; height:{size}px; \
                                         border-radius:50%; background:{color}; cursor:pointer; \
                                         border:{};",
                                        it.x * 100.0,
                                        (1.0 - it.y) * 100.0,
                                        if sel { "2px solid #ffffff" } else { "1px solid #00000055" },
                                    ),
                                    onclick: move |_| pick(it2.clone()),
                                }
                            }
                        }
                    }
                }
            }
            // ── similarity side list ──
            div {
                style: "width:280px; display:flex; flex-direction:column; gap:8px; \
                        border-left:1px solid #262b38; padding-left:12px; min-height:0;",
                if let Some(sel) = selected() {
                    div {
                        style: "font-weight:600; font-size:13px; word-break:break-all;",
                        span { style: format!("color:{};", class_color(&sel.class)), "● " }
                        "{sel.path}"
                    }
                    div {
                        style: "font-size:11px; color:#8a8fa3;",
                        {format!("{} · {:.0} Hz · {:.2}s", sel.class, sel.centroid_hz, sel.duration_s)}
                    }
                    div { style: "font-size:12px; color:#8a8fa3; margin-top:6px;", "similar:" }
                    div {
                        style: "display:flex; flex-direction:column; gap:4px; overflow-y:auto; min-height:0;",
                        for hit in similar() {
                            {
                                let color = class_color(&hit.class);
                                let as_item = items().into_iter().find(|i| i.idx == hit.idx);
                                rsx! {
                                    button {
                                        style: "text-align:left; padding:4px 6px; border-radius:6px; \
                                                border:1px solid #262b38; background:#1a1e28; \
                                                color:#e6e8ef; cursor:pointer; font-size:12px; \
                                                word-break:break-all;",
                                        onclick: move |_| {
                                            if let Some(it) = as_item.clone() {
                                                pick(it);
                                            }
                                        },
                                        span { style: format!("color:{color};"), "● " }
                                        {format!("{:.2}  {}", hit.score, hit.path)}
                                    }
                                }
                            }
                        }
                    }
                } else {
                    div {
                        style: "color:#8a8fa3; font-size:12px;",
                        "Click a dot to audition it and see its nearest neighbours."
                    }
                }
            }
        }
    }
}
