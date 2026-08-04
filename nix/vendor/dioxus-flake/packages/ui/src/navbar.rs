use dioxus::prelude::*;

#[component]
pub fn Navbar(children: Element) -> Element {
    rsx! {
        nav { class: "flex items-center gap-6 px-6 py-4 bg-gray-900 border-b border-gray-800",
            {children}
        }
    }
}
