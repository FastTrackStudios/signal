use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn Blog(id: i32) -> Element {
    rsx! {
        div { class: "min-h-screen bg-gray-950 text-white p-8 flex flex-col items-center",
            div { class: "max-w-2xl w-full",
                h1 { class: "text-3xl font-bold mb-4",
                    "Blog Post #{id}"
                }
                p { class: "text-gray-400 mb-8 leading-relaxed",
                    "In blog #{id}, we show how the Dioxus router works and how URL parameters can be passed as props to our route components."
                }
                div { class: "flex items-center gap-4",
                    Link {
                        to: Route::Blog { id: id - 1 },
                        class: "px-4 py-2 bg-blue-600 rounded-lg hover:bg-blue-500 transition-colors",
                        "Previous"
                    }
                    Link {
                        to: Route::Blog { id: id + 1 },
                        class: "px-4 py-2 bg-blue-600 rounded-lg hover:bg-blue-500 transition-colors",
                        "Next"
                    }
                }
            }
        }
    }
}
