use dioxus_native::prelude::*;
use ui::Showcase;

fn main() {
    dioxus_native::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        Stylesheet { href: asset!("/assets/tailwind.css") }
        Showcase {}
    }
}
