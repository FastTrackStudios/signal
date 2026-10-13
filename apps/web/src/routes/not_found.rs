use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let _ = segments;
    rsx! {
        div { class: "h-screen w-screen flex flex-col items-center justify-center gap-4 bg-zinc-950 text-zinc-100",
            h1 { class: "text-2xl font-bold", "Not found" }
            Link {
                to: Route::Home {},
                class: "rounded-md bg-sky-500 px-5 py-2.5 font-medium text-zinc-950 hover:bg-sky-400 transition-colors",
                "Back home"
            }
        }
    }
}
