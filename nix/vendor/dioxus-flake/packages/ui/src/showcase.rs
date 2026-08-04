use dioxus::prelude::*;

use crate::{Hero, Navbar};

#[component]
pub fn Showcase() -> Element {
    let mut count = use_signal(|| 0);

    rsx! {
        div { class: "min-h-screen bg-gray-950 text-white",
            Navbar {
                a { class: "text-white hover:text-blue-400 transition-colors", href: "#hero", "Hero" }
                a { class: "text-white hover:text-blue-400 transition-colors", href: "#controls", "Controls" }
                a { class: "text-white hover:text-blue-400 transition-colors", href: "#cards", "Cards" }
            }

            main { class: "mx-auto flex max-w-6xl flex-col gap-10 px-6 py-8",
                section { id: "hero", class: "rounded-xl border border-gray-800 bg-gray-900/60 p-6",
                    Hero {}
                }

                section { id: "controls", class: "grid gap-4 rounded-xl border border-gray-800 bg-gray-900/60 p-6",
                    h2 { class: "text-2xl font-semibold", "Interactive Controls" }
                    p { class: "text-sm text-gray-400",
                        "This shared showcase is rendered by web, desktop, mobile, and native packages."
                    }
                    div { class: "flex flex-wrap items-center gap-3",
                        button {
                            class: "rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500",
                            onclick: move |_| count += 1,
                            "Increment"
                        }
                        button {
                            class: "rounded-lg border border-gray-700 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-gray-800",
                            onclick: move |_| count -= 1,
                            "Decrement"
                        }
                        span { class: "rounded-lg bg-gray-800 px-4 py-2 text-sm", "Count: {count}" }
                    }
                }

                section { id: "cards", class: "grid gap-4 md:grid-cols-3",
                    ShowcaseCard {
                        title: "Web",
                        body: "Browser rendering through Dioxus web."
                    }
                    ShowcaseCard {
                        title: "Desktop",
                        body: "Webview rendering through Dioxus desktop."
                    }
                    ShowcaseCard {
                        title: "Native",
                        body: "Blitz native rendering without a webview."
                    }
                }
            }
        }
    }
}

#[component]
fn ShowcaseCard(title: String, body: String) -> Element {
    rsx! {
        article { class: "rounded-xl border border-gray-800 bg-gray-900 p-5 shadow-sm",
            h3 { class: "mb-2 text-lg font-semibold", "{title}" }
            p { class: "text-sm text-gray-400", "{body}" }
        }
    }
}
