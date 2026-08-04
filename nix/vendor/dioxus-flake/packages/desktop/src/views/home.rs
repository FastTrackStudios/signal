use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    rsx! {
        div { class: "min-h-screen bg-gray-950 text-white flex flex-col items-center justify-center p-8",
            h1 { class: "text-5xl font-bold mb-4 bg-gradient-to-r from-blue-400 to-purple-500 bg-clip-text text-transparent",
                "Dioxus + Tailwind"
            }
            p { class: "text-gray-400 text-lg mb-8 max-w-md text-center",
                "A multi-platform app template with web, desktop, and mobile targets — powered by Nix."
            }
            div { class: "grid grid-cols-2 gap-4",
                a {
                    href: "https://dioxuslabs.com/learn/0.7/",
                    class: "px-6 py-3 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors text-center",
                    "Learn Dioxus"
                }
                a {
                    href: "https://tailwindcss.com/docs",
                    class: "px-6 py-3 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors text-center",
                    "Tailwind Docs"
                }
                a {
                    href: "https://github.com/DioxusLabs/dioxus",
                    class: "px-6 py-3 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors text-center",
                    "GitHub"
                }
                a {
                    href: "https://discord.gg/XgGxMSkvUM",
                    class: "px-6 py-3 bg-gray-800 rounded-lg hover:bg-gray-700 transition-colors text-center",
                    "Discord"
                }
            }
        }
    }
}
