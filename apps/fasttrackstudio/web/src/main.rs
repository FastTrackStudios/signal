//! FastTrackStudio Documentation Site
//!
//! Interactive documentation with live chart rendering for the keyflow parser.
//! Test patterns are displayed as interactive examples using WebGPU.

mod components;
mod renderer;
mod state;

use dioxus::prelude::*;
use fts_ui::prelude::*;
use fts_ui::lucide_dioxus::{
    ArrowLeft, BookOpen, ChevronDown, ChevronLeft, ChevronRight, Circle, Code, ExternalLink,
    FileCode, FileText, Github, ListMusic, Music, PenLine, PenTool, Play, SkipBack, SkipForward,
    Square, User, Users,
};

use audio_controls::widgets::{
    CompressorGraph, CompressorMetering, CompressorMode, CompressorParams, CompressorWidget, DbRange,
    EqBand, EqBandShape, EqGraph,
    GateDbRange, GateGraph, GateMetering, GateMode, GateParams,
};

// Static assets
const FAVICON: Asset = asset!("/assets/favicon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

/// Application routes
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Layout)]
    #[route("/")]
    Home {},
    #[route("/docs")]
    DocsHome {},
    #[route("/keyflow/chart")]
    ChartEditor {},
    // Keyflow docs
    #[route("/docs/keyflow")]
    DocsKeyflow {},
    #[route("/docs/keyflow/snippets")]
    SnippetsBrowser {},
    #[route("/docs/keyflow/snippets/:id")]
    SnippetsView { id: String },
    // Other docs sections
    #[route("/docs/reaper")]
    DocsReaper {},
    #[route("/docs/desktop")]
    DocsDesktop {},
    #[route("/docs/plugins")]
    DocsPlugins {},
    // Legacy routes - redirect to new ones
    #[route("/docs/keyflow/chart/tests")]
    PatternBrowser {},
    #[route("/docs/keyflow/chart/tests/:id")]
    PatternView { id: String },
    #[route("/test/render")]
    TestRender {},
    #[route("/test/fx-ui")]
    TestFxUi {},
}

fn main() {
    // Initialize panic hook for better WASM debugging
    #[cfg(target_arch = "wasm32")]
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        console_error_panic_hook::set_once();

        let filter = tracing_subscriber::EnvFilter::new(
            "warn,fasttrackstudio_web=debug,keyflow=info,keyflow::engraver=debug",
        );

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_wasm::WASMLayer::new(
                tracing_wasm::WASMLayerConfig::default(),
            ))
            .init();
    }

    // Initialize logging for non-WASM
    #[cfg(not(target_arch = "wasm32"))]
    {
        use tracing_subscriber::EnvFilter;
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("warn,fasttrackstudio_web=debug,keyflow=info")),
            )
            .init();
    }

    tracing::info!("Starting FastTrackStudio Documentation Site");

    dioxus::launch(App);
}

/// Root application component
#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        Router::<Route> {}
    }
}

/// Main layout with navigation
#[component]
fn Layout() -> Element {
    let route = use_route::<Route>();
    let is_home = matches!(route, Route::Home {});

    rsx! {
        div {
            class: "min-h-screen flex flex-col text-foreground",

            // Floating glass navbar
            nav {
            class: if is_home {
                "fixed top-0 left-0 right-0 z-50"
            } else {
                "sticky top-0 z-50 bg-background/80 backdrop-blur-xl border-b border-border/50"
            },

            div {
                class: "max-w-7xl mx-auto px-4 lg:px-8",

                div {
                    class: if is_home {
                        "flex items-center justify-between py-4 px-6 my-4 rounded-2xl bg-card/60 backdrop-blur-xl border border-white/[0.08] shadow-lg shadow-black/5"
                    } else {
                        "flex items-center justify-between py-3"
                    },

                    // Logo with icon
                    Link {
                        to: Route::Home {},
                        class: "flex items-center gap-3 group",

                        // Logo icon - stylized "F" with music note aesthetic
                        div {
                            class: "w-8 h-8 rounded-lg bg-gradient-to-br from-primary to-primary/60 flex items-center justify-center shadow-lg shadow-primary/20 group-hover:shadow-primary/40 transition-shadow",
                            span {
                                class: "text-primary-foreground font-black text-sm",
                                "F"
                            }
                        }

                        div {
                            class: "flex flex-col",
                            span {
                                class: "text-foreground font-semibold text-sm leading-tight group-hover:text-primary transition-colors",
                                "FastTrackStudio"
                            }
                            // Show section name based on current route
                            {
                                let section = match route {
                                    Route::ChartEditor {} => Some("Keyflow Editor"),
                                    Route::DocsHome {}
                                    | Route::DocsKeyflow {}
                                    | Route::DocsReaper {}
                                    | Route::DocsDesktop {}
                                    | Route::DocsPlugins {}
                                    | Route::SnippetsBrowser {}
                                    | Route::SnippetsView { .. }
                                    | Route::PatternBrowser {}
                                    | Route::PatternView { .. } => Some("Documentation"),
                                    _ => None,
                                };
                                if let Some(name) = section {
                                    rsx! {
                                        span {
                                            class: "text-muted-foreground text-xs leading-tight",
                                            "{name}"
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            }
                        }
                    }

                    // Center navigation (hidden on mobile)
                    div {
                        class: "hidden md:flex items-center gap-1 px-1.5 py-1.5 rounded-xl bg-muted/50",

                        NavLink {
                            to: Route::DocsHome {},
                            icon: rsx! { BookOpen { class: "w-4 h-4" } },
                            label: "Docs"
                        }

                        NavLink {
                            to: Route::ChartEditor {},
                            icon: rsx! { PenTool { class: "w-4 h-4" } },
                            label: "Editor"
                        }

                        NavLink {
                            to: Route::SnippetsBrowser {},
                            icon: rsx! { FileCode { class: "w-4 h-4" } },
                            label: "Snippets"
                        }
                    }

                    // Right side actions
                    div {
                        class: "flex items-center gap-2",

                        // GitHub link
                        a {
                            href: "https://github.com/codywright/FastTrackStudio",
                            target: "_blank",
                            class: "p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-all",
                            title: "View on GitHub",
                            Github { class: "w-5 h-5" }
                        }

                        // Primary CTA button
                        Link {
                            to: Route::ChartEditor {},
                            class: "hidden sm:flex items-center gap-2 px-4 py-2 rounded-lg bg-primary text-primary-foreground font-medium text-sm hover:bg-primary/90 transition-colors shadow-lg shadow-primary/20 hover:shadow-primary/30",
                            PenTool { class: "w-4 h-4" }
                            span { "Try Editor" }
                        }
                    }
                }
            }
            }

            // Main content area
            main {
                class: if is_home { "flex-1" } else { "flex-1" },
                Outlet::<Route> {}
            }
        }
    }
}

/// Navigation link component with active state
#[component]
fn NavLink(to: Route, icon: Element, label: &'static str) -> Element {
    let current_route = use_route::<Route>();
    let is_active = std::mem::discriminant(&current_route) == std::mem::discriminant(&to);

    rsx! {
        Link {
            to: to,
            class: if is_active {
                "flex items-center gap-2 px-3 py-1.5 rounded-lg text-foreground bg-background shadow-sm text-sm font-medium transition-all"
            } else {
                "flex items-center gap-2 px-3 py-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-background/50 text-sm transition-all"
            },
            {icon}
            span { "{label}" }
        }
    }
}

/// Chart examples for the typewriter animation on the landing page.
const DEMO_CHARTS: &[&str] = &[
    // Complex funk/jazz - Thriller (Dirty Loops)
    r#"Thriller
120bpm 4/4 #Eb
/push = triplet

HITS
r8t >Ab9_8t r8t r8t r8t >F9_8t r2
s1

IN
>'Cm . . .

VS
>'F/C . Cm .

CH
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // |
'F/A //// | 'Fm9  ////
>Cm/Eb / 'Eb /// | 'Eb / 'F/C / 'Cm // | 'F/A |
r8t >Ab9_8t r8t r8t >'F9_8t r8t r4 >Fm/Ab_4

INST
Cm . F6 // Abdim7 'Csus2
"#,
    // Pop ballad in A major
    r#"Midnight Dreams
72bpm 4/4 #A

VS
A D F#m E

CH
D E A F#m
D E C#m F#m

BR
F#m E D A/C#

OUT
A
"#,
    // Jazz standard in Bb
    r#"Autumn Leaves
140bpm 4/4 #Bb

VS
Cm7 F7 Bbmaj7 Ebmaj7
Am7b5 D7 Gm //

CH
Am7b5 D7 Gm //
Cm7 F7 Bbmaj7 //

OUT
Gm
"#,
    // R&B groove in C minor
    r#"City Lights
95bpm 4/4 #Cm

VS
Cm9 Fm9 Bb13 Ebmaj7
Abmaj7 G7#9 Cm9 //

CH
Fm9 G7 Cm9 //
Abmaj7 Bb9 Cm9 //

BR
Abmaj7 G7 Cm //

OUT
Cm9
"#,
];

/// Landing page for FastTrackStudio
#[component]
fn Home() -> Element {
    // Source text state driven by typewriter animation
    let source = use_signal(|| DEMO_CHARTS[0].to_string());
    // Use Page mode for proper A4 document appearance
    let preview_mode = use_signal(|| components::PreviewMode::Page);

    // Create the list of charts for the typewriter
    let charts: Vec<String> = DEMO_CHARTS.iter().map(|s| s.to_string()).collect();

    rsx! {
        div {
            class: "relative pt-24",

            // Decorative gradient overlays - diagonal beams like reference
            div {
                aria_hidden: "true",
                class: "z-[2] absolute inset-0 pointer-events-none isolate hidden lg:block overflow-hidden",

                // Main beam - upper left
                div {
                    class: "absolute left-0 top-0 rounded-full",
                    style: "width: 35rem; height: 80rem; transform: translateY(-87.5%) rotate(-45deg); background: radial-gradient(68.54% 68.72% at 55.02% 31.46%, hsla(0,0%,85%,.04) 0, hsla(0,0%,55%,.01) 50%, hsla(0,0%,45%,0) 80%);"
                }

                // Secondary beam
                div {
                    class: "absolute left-0 top-0 rounded-full",
                    style: "width: 14rem; height: 80rem; transform: translate(5%, -50%) rotate(-45deg); background: radial-gradient(50% 50% at 50% 50%, hsla(0,0%,85%,.03) 0, hsla(0,0%,45%,.01) 80%, transparent 100%);"
                }

                // Third beam
                div {
                    class: "absolute left-0 top-0 rounded-full",
                    style: "width: 14rem; height: 80rem; transform: translateY(-87.5%) rotate(-45deg); background: radial-gradient(50% 50% at 50% 50%, hsla(0,0%,85%,.02) 0, hsla(0,0%,45%,.01) 80%, transparent 100%);"
                }

                // Additional beam - center
                div {
                    class: "absolute rounded-full",
                    style: "width: 20rem; height: 60rem; left: 30%; top: 0; transform: translateY(-60%) rotate(-45deg); background: radial-gradient(50% 50% at 50% 50%, hsla(0,0%,85%,.025) 0, hsla(0,0%,45%,.008) 80%, transparent 100%);"
                }

                // Additional beam - right side
                div {
                    class: "absolute rounded-full",
                    style: "width: 18rem; height: 50rem; right: 10%; top: 0; transform: translateY(-40%) rotate(-45deg); background: radial-gradient(50% 50% at 50% 50%, hsla(0,0%,85%,.02) 0, hsla(0,0%,45%,.006) 80%, transparent 100%);"
                }

                // Subtle beam - lower
                div {
                    class: "absolute rounded-full",
                    style: "width: 12rem; height: 40rem; left: 50%; top: 30%; transform: rotate(-45deg); background: radial-gradient(50% 50% at 50% 50%, hsla(0,0%,85%,.015) 0, transparent 80%);"
                }
            }

            // Hero section
            section {
                class: "overflow-hidden",

                div {
                    class: "relative mx-auto max-w-5xl px-6 py-28 lg:py-24",

                    div {
                        class: "relative z-10 mx-auto max-w-2xl text-center",

                        h1 {
                            class: "text-balance text-4xl font-semibold md:text-5xl lg:text-6xl text-foreground",
                            "Fast-Efficient Workflow"
                        }

                        p {
                            class: "mx-auto my-8 max-w-2xl text-lg md:text-xl text-muted-foreground",
                            "A complete music production toolset built in Rust. Seamless REAPER integration, powerful chart notation, and real-time P2P collaboration."
                        }

                        div {
                            class: "flex flex-col sm:flex-row items-center justify-center gap-3",

                            Link {
                                to: Route::ChartEditor {},
                                class: "inline-flex items-center justify-center gap-2 bg-primary text-primary-foreground hover:bg-primary/90 h-11 px-8 rounded-md font-medium transition-colors",
                                "Try the Editor"
                            }

                            Link {
                                to: Route::DocsHome {},
                                class: "inline-flex items-center justify-center gap-2 border border-border bg-background hover:bg-accent hover:text-accent-foreground h-11 px-8 rounded-md font-medium transition-colors",
                                "Read the Docs"
                            }
                        }
                    }
                }
            }

            // 3D Carousel — outside the overflow-hidden hero section so sticky works
            ShowcaseCarousel {
                source: source,
                preview_mode: preview_mode,
                charts: charts.clone()
            }

            // Documentation sections
            section {
                class: "relative z-10 py-24",

                div {
                    class: "mx-auto max-w-5xl px-6",

                    h2 {
                        class: "text-center text-lg font-medium text-muted-foreground mb-4",
                        "Everything you need for modern music production"
                    }

                    p {
                        class: "text-center text-3xl font-semibold text-foreground mb-12",
                        "Explore the Toolkit"
                    }

                    // 4 main documentation sections
                    div {
                        class: "grid md:grid-cols-2 gap-6",

                        LandingDocsCard {
                            to: Route::DocsKeyflow {},
                            title: "Keyflow",
                            description: "Chart notation language with GPU rendering",
                            icon: rsx! { FileText { class: "w-6 h-6" } },
                            color: "emerald"
                        }

                        LandingDocsCard {
                            to: Route::DocsReaper {},
                            title: "REAPER Extension",
                            description: "Deep DAW integration and transport sync",
                            icon: rsx! { Music { class: "w-6 h-6" } },
                            color: "violet"
                        }

                        LandingDocsCard {
                            to: Route::DocsDesktop {},
                            title: "Desktop App",
                            description: "Setlists, lyrics, and live performance",
                            icon: rsx! { fts_ui::lucide_dioxus::Monitor { class: "w-6 h-6" } },
                            color: "blue"
                        }

                        LandingDocsCard {
                            to: Route::DocsPlugins {},
                            title: "Audio Plugins",
                            description: "CLAP and VST3 with nih-plug",
                            icon: rsx! { fts_ui::lucide_dioxus::SlidersHorizontal { class: "w-6 h-6" } },
                            color: "amber"
                        }
                    }
                }
            }

            // Features highlights section
            section {
                class: "relative z-10 py-16 border-t border-border/30",

                div {
                    class: "mx-auto max-w-5xl px-6",

                    div {
                        class: "grid md:grid-cols-3 gap-8",

                        HomeFeature {
                            title: "REAPER Integration",
                            description: "Deep DAW integration for transport control, MIDI routing, and real-time state synchronization.",
                            icon: rsx! { Music { class: "w-8 h-8" } }
                        }

                        HomeFeature {
                            title: "Keyflow Notation",
                            description: "Intuitive chart syntax with smart chord memory, section numbering, and GPU-accelerated rendering.",
                            icon: rsx! { FileText { class: "w-8 h-8" } }
                        }

                        HomeFeature {
                            title: "P2P Collaboration",
                            description: "Real-time collaboration powered by iroh for seamless peer-to-peer networking.",
                            icon: rsx! { Users { class: "w-8 h-8" } }
                        }
                    }
                }
            }

            // Setlist Control Preview section
            section {
                class: "relative z-10 py-24 bg-card/30",

                div {
                    class: "mx-auto max-w-5xl px-6",

                    div {
                        class: "text-center mb-12",
                        h2 {
                            class: "text-3xl font-bold text-foreground mb-4",
                            "Live Performance Control"
                        }
                        p {
                            class: "text-lg text-muted-foreground max-w-2xl mx-auto",
                            "Navigate your setlist with ease. Real-time section tracking, song navigation, and transport controls all in one unified view."
                        }
                    }

                    SetlistPreviewCard {}
                }
            }
        }
    }
}

/// Preview card for the setlist control feature on the landing page
#[component]
fn SetlistPreviewCard() -> Element {
    rsx! {
        div {
            class: "relative rounded-2xl border border-border/50 bg-card overflow-hidden",

            // Mock window chrome
            div {
                class: "flex items-center gap-2 px-4 py-3 border-b border-border bg-zinc-900/80",

                div { class: "w-3 h-3 rounded-full bg-red-500/80" }
                div { class: "w-3 h-3 rounded-full bg-yellow-500/80" }
                div { class: "w-3 h-3 rounded-full bg-green-500/80" }

                span {
                    class: "ml-4 text-sm text-muted-foreground",
                    "Setlist Control"
                }
            }

            // Preview of the setlist view (constrained height)
            div {
                class: "h-96 overflow-hidden flex items-center justify-center bg-muted/30 rounded-lg",
                div { class: "text-center text-muted-foreground",
                    p { "Performance View Preview" }
                    p { class: "text-sm mt-2 opacity-50", "(Coming soon)" }
                }
            }

            // Overlay with CTA
            div {
                class: "absolute bottom-0 left-0 right-0 bg-gradient-to-t from-card via-card/95 to-transparent pt-16 pb-6 px-6 text-center",

                Link {
                    to: Route::DocsDesktop {},
                    class: "inline-flex items-center justify-center gap-2 bg-primary text-primary-foreground hover:bg-primary/90 h-11 px-8 rounded-md font-medium transition-colors",
                    ListMusic { class: "w-5 h-5" }
                    "Learn More"
                }
            }
        }
    }
}

#[component]
fn HomeFeature(title: &'static str, description: &'static str, icon: Element) -> Element {
    rsx! {
        Card {
            class: "group relative border-border/50 bg-card/30 p-8 transition-all hover:border-primary/30 hover:bg-card/50",

            div {
                class: "mb-4 inline-flex rounded-lg bg-primary/10 p-3 text-primary",
                {icon}
            }

            Heading { level: HeadingLevel::H5, class: "text-foreground mb-2", "{title}" }

            Text { variant: TextVariant::Muted, class: "leading-relaxed", "{description}" }
        }
    }
}

/// Landing page docs section card with colored accent
#[component]
fn LandingDocsCard(
    to: Route,
    title: &'static str,
    description: &'static str,
    icon: Element,
    color: &'static str,
) -> Element {
    let (bg_color, border_color, text_color) = match color {
        "emerald" => (
            "bg-emerald-500/10",
            "hover:border-emerald-500/50 hover:shadow-emerald-500/5",
            "text-emerald-500",
        ),
        "violet" => (
            "bg-violet-500/10",
            "hover:border-violet-500/50 hover:shadow-violet-500/5",
            "text-violet-500",
        ),
        "blue" => (
            "bg-blue-500/10",
            "hover:border-blue-500/50 hover:shadow-blue-500/5",
            "text-blue-500",
        ),
        "amber" => (
            "bg-amber-500/10",
            "hover:border-amber-500/50 hover:shadow-amber-500/5",
            "text-amber-500",
        ),
        _ => ("bg-primary/10", "hover:border-primary/50", "text-primary"),
    };

    rsx! {
        Link {
            to: to,
            class: "group flex items-center gap-4 p-5 rounded-xl border border-border/50 bg-card/30 transition-all {border_color} hover:shadow-lg hover:bg-card/50",

            div {
                class: "shrink-0 rounded-lg p-2.5 {bg_color} {text_color}",
                {icon}
            }

            div {
                class: "min-w-0",
                h3 {
                    class: "font-semibold text-foreground group-hover:{text_color} transition-colors",
                    "{title}"
                }
                p {
                    class: "text-sm text-muted-foreground truncate",
                    "{description}"
                }
            }

            ChevronRight {
                class: "w-5 h-5 text-muted-foreground/50 group-hover:text-muted-foreground ml-auto shrink-0 transition-colors"
            }
        }
    }
}

// =============================================================================
// Horizontal Showcase Strip with Perspective
// =============================================================================

/// Number of cards in the showcase
const SHOWCASE_CARD_COUNT: usize = 5;

/// Individual card widths in pixels
const CARD_WIDTHS: [i32; 5] = [724, 480, 480, 480, 480];

/// Gap between cards in pixels
const CARD_GAP: i32 = 24;

/// Total horizontal travel distance for the carousel strip (px).
const CAROUSEL_TRAVEL: i32 = {
    let mut w = 0;
    let mut i = 0;
    while i < SHOWCASE_CARD_COUNT {
        w += CARD_WIDTHS[i];
        if i < SHOWCASE_CARD_COUNT - 1 {
            w += CARD_GAP;
        }
        i += 1;
    }
    w - CARD_WIDTHS[0]
};

/// Horizontal showcase strip driven by page scroll position.
///
/// The section is taller than the viewport — the extra height is "scroll runway."
/// The visible carousel is `position: sticky` so it stays pinned while you scroll
/// through the section. A window scroll listener maps the section's scroll
/// progress (0..1) to the strip's horizontal translateX.
///
/// When you scroll past the section, normal page scrolling continues seamlessly.
#[component]
fn ShowcaseCarousel(
    source: Signal<String>,
    preview_mode: Signal<components::PreviewMode>,
    charts: Vec<String>,
) -> Element {
    let n = SHOWCASE_CARD_COUNT;
    let mut offset = use_signal(|| 0.0_f64);
    let px = *offset.read() as i32;

    // Set up a window scroll listener to drive the carousel position.
    // The listener reads the section element's position relative to viewport
    // and computes how far through the "runway" we've scrolled.
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();

        let closure = Closure::<dyn FnMut()>::new(move || {
            let Some(window) = web_sys::window() else { return };
            let Some(document) = window.document() else { return };
            let Some(section) = document.get_element_by_id("showcase-scroll-section") else { return };
            let rect = section.get_bounding_client_rect();
            let section_height = rect.height();
            let viewport_h = window.inner_height().unwrap().as_f64().unwrap();

            let runway = section_height - viewport_h;
            if runway <= 0.0 { return; }

            let scrolled = -rect.top();
            let progress = (scrolled / runway).clamp(0.0, 1.0);

            offset.set(-(progress * CAROUSEL_TRAVEL as f64));
        });

        // add_event_listener accepts &Function — FnMut closures work via as_ref()
        let _ = window.add_event_listener_with_callback("scroll", closure.as_ref().unchecked_ref());

        // Keep closure alive for the component's lifetime
        // (it'll be dropped when the component unmounts and the effect re-runs)
        std::mem::forget(closure);
    });

    // Section height = viewport + runway. A shorter runway means the carousel
    // scrolls through faster, keeping the hero text and content below visible.
    let runway_px = CAROUSEL_TRAVEL / 2;
    let section_style = format!("height: calc(100vh + {runway_px}px);");

    rsx! {
        div {
            id: "showcase-scroll-section",
            class: "-mt-8 relative w-full",
            style: "{section_style}",

            // Sticky container — pinned with vertical centering offset
            div {
                class: "sticky top-[15vh] w-full overflow-hidden",

                // Cards with bottom + right fade masks
                div {
                    class: "w-full",
                    style: "-webkit-mask-image: linear-gradient(to bottom, black 40%, transparent 100%); mask-image: linear-gradient(to bottom, black 40%, transparent 100%);",

                    div {
                        style: "perspective: 1200px; -webkit-mask-image: linear-gradient(to right, black 50%, transparent 100%); mask-image: linear-gradient(to right, black 50%, transparent 100%);",

                        div {
                            class: "pl-8 md:pl-16 lg:pl-32",
                            style: "transform: rotateX(20deg) skewX(0.36rad);",

                            div {
                                class: "h-[44rem] lg:h-[52rem]",

                                div {
                                    class: "flex",
                                    style: "gap: {CARD_GAP}px; transform: translateX({px}px);",

                                    for i in 0..n {
                                        {
                                            let card_width = CARD_WIDTHS[i];
                                            rsx! {
                                                div {
                                                    key: "{i}",
                                                    class: "shrink-0",
                                                    style: "width: {card_width}px;",

                                                    match i {
                                                        0 => rsx! {
                                                            KeyflowShowcaseCard {
                                                                source: source,
                                                                preview_mode: preview_mode,
                                                                charts: charts.clone()
                                                            }
                                                        },
                                                        1 => rsx! { DesktopShowcaseCard {} },
                                                        2 => rsx! { ReaperShowcaseCard {} },
                                                        3 => rsx! { PluginsShowcaseCard {} },
                                                        _ => rsx! { CollaborationShowcaseCard {} },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Keyflow Editor showcase card
#[component]
fn KeyflowShowcaseCard(
    source: Signal<String>,
    preview_mode: Signal<components::PreviewMode>,
    charts: Vec<String>,
) -> Element {
    rsx! {
        div {
            class: "rounded-lg border border-border bg-card overflow-hidden shadow-2xl w-full",
            style: "height: 600px;",

            // Window header
            div {
                class: "flex items-center gap-2 px-4 py-3 border-b border-border bg-zinc-900/80",

                div { class: "w-3 h-3 rounded-full bg-red-500/80" }
                div { class: "w-3 h-3 rounded-full bg-yellow-500/80" }
                div { class: "w-3 h-3 rounded-full bg-green-500/80" }

                span {
                    class: "ml-4 text-sm text-muted-foreground",
                    "song_title.kf"
                }

                span {
                    class: "ml-auto text-xs text-emerald-500 font-medium px-2 py-0.5 rounded bg-emerald-500/10",
                    "Keyflow"
                }
            }

            // Typewriter animation (hidden, drives source signal)
            components::ChartTypewriter {
                output: source,
                charts: charts,
                speed_ms: 35,
                delay_between_charts_ms: 4000
            }

            // Split view content
            div {
                class: "flex h-[calc(100%-3rem)]",

                // Editor side
                div {
                    class: "w-56 border-r border-border overflow-hidden shrink-0",

                    components::HighlightedEditor {
                        value: source(),
                        on_change: move |_: String| {},
                        placeholder: "",
                        textarea_id: Some("carousel-editor".to_string())
                    }
                }

                // Preview side
                div {
                    class: "overflow-hidden flex-1",

                    components::StaticChartRenderer {
                        source: source,
                        mode: preview_mode,
                        canvas_id: Some("carousel-chart-canvas".to_string()),
                        fixed_layout_width: Some(180.0)
                    }
                }
            }
        }
    }
}

/// Desktop App showcase card with tempo UI
#[component]
fn DesktopShowcaseCard() -> Element {
    let tempo = 120;

    rsx! {
        div {
            class: "rounded-lg border border-border bg-card overflow-hidden shadow-2xl w-full",
            style: "height: 600px;",

            div {
                class: "flex items-center gap-2 px-4 py-3 border-b border-border bg-zinc-900/80",
                div { class: "w-3 h-3 rounded-full bg-red-500/80" }
                div { class: "w-3 h-3 rounded-full bg-yellow-500/80" }
                div { class: "w-3 h-3 rounded-full bg-green-500/80" }
                span { class: "ml-4 text-sm text-muted-foreground", "FastTrackStudio" }
                span { class: "ml-auto text-xs text-blue-500 font-medium px-2 py-0.5 rounded bg-blue-500/10", "Desktop" }
            }

            div {
                class: "relative h-[calc(100%-3rem)] bg-zinc-900 flex flex-col items-center justify-center",
                span { class: "text-xs uppercase tracking-wider text-muted-foreground mb-3", "Tempo" }
                div {
                    class: "flex items-baseline",
                    span { class: "text-9xl font-bold tabular-nums text-foreground", "{tempo}" }
                    span { class: "text-3xl font-medium text-muted-foreground ml-3", "BPM" }
                }
                div {
                    class: "flex items-center gap-4 mt-10",
                    for i in 0..4 {
                        div {
                            key: "{i}",
                            class: if i == 0 { "w-5 h-5 rounded-full bg-primary animate-pulse" } else { "w-4 h-4 rounded-full bg-muted-foreground/30" }
                        }
                    }
                }
                div {
                    class: "absolute bottom-8 flex items-center gap-10 text-muted-foreground/50",
                    SkipBack { class: "w-7 h-7" }
                    div {
                        class: "p-4 rounded-full bg-primary/20 text-primary",
                        Play { class: "w-10 h-10" }
                    }
                    SkipForward { class: "w-7 h-7" }
                }
            }
        }
    }
}

/// REAPER Extension showcase card
#[component]
fn ReaperShowcaseCard() -> Element {
    rsx! {
        div {
            class: "rounded-lg border border-border bg-card overflow-hidden shadow-2xl w-full",
            style: "height: 600px;",

            div {
                class: "flex items-center gap-2 px-4 py-3 border-b border-border bg-zinc-900/80",
                div { class: "w-3 h-3 rounded-full bg-red-500/80" }
                div { class: "w-3 h-3 rounded-full bg-yellow-500/80" }
                div { class: "w-3 h-3 rounded-full bg-green-500/80" }
                span { class: "ml-4 text-sm text-muted-foreground", "REAPER" }
                span { class: "ml-auto text-xs text-violet-500 font-medium px-2 py-0.5 rounded bg-violet-500/10", "Extension" }
            }

            div {
                class: "relative h-[calc(100%-3rem)] bg-zinc-900 p-5",

                div {
                    class: "flex items-center gap-4 mb-5 p-4 rounded-lg bg-zinc-800/50 border border-border/50",
                    div {
                        class: "flex items-center gap-2",
                        div { class: "w-10 h-10 rounded bg-red-500/20 flex items-center justify-center text-red-500",
                            Circle { class: "w-5 h-5 fill-current" }
                        }
                        div { class: "w-10 h-10 rounded bg-primary/20 flex items-center justify-center text-primary",
                            Play { class: "w-5 h-5" }
                        }
                        div { class: "w-10 h-10 rounded bg-zinc-700 flex items-center justify-center text-muted-foreground",
                            Square { class: "w-5 h-5" }
                        }
                    }
                    div { class: "ml-4 font-mono text-xl text-foreground tabular-nums", "00:01:24.15" }
                    div { class: "ml-auto text-sm text-muted-foreground", "4/4 • 120 BPM" }
                }

                div {
                    class: "space-y-3 flex-1",
                    for i in 0..6 {
                        div {
                            key: "{i}",
                            class: "flex items-center gap-3 p-3 rounded bg-zinc-800/30 border border-border/30",
                            div {
                                class: "w-20 text-xs text-muted-foreground truncate",
                                { match i {
                                    0 => "Drums",
                                    1 => "Bass",
                                    2 => "Keys",
                                    3 => "Guitar",
                                    4 => "Synth",
                                    _ => "Vocals"
                                }}
                            }
                            div {
                                class: "flex-1 h-10 rounded bg-zinc-700/50 overflow-hidden flex items-center gap-px px-1",
                                for j in 0..50 {
                                    div {
                                        key: "{j}",
                                        class: "w-1 bg-violet-500/40 rounded-full",
                                        style: "height: {((j * 7 + i * 13) % 24 + 6)}px;"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Audio Plugins showcase card
#[component]
fn PluginsShowcaseCard() -> Element {
    rsx! {
        div {
            class: "rounded-lg border border-border bg-card overflow-hidden shadow-2xl w-full",
            style: "height: 600px;",

            div {
                class: "flex items-center gap-2 px-4 py-3 border-b border-border bg-zinc-900/80",
                div { class: "w-3 h-3 rounded-full bg-red-500/80" }
                div { class: "w-3 h-3 rounded-full bg-yellow-500/80" }
                div { class: "w-3 h-3 rounded-full bg-green-500/80" }
                span { class: "ml-4 text-sm text-muted-foreground", "fts-gain" }
                span { class: "ml-auto text-xs text-amber-500 font-medium px-2 py-0.5 rounded bg-amber-500/10", "VST3 / CLAP" }
            }

            div {
                class: "relative h-[calc(100%-3rem)] bg-gradient-to-b from-zinc-900 to-zinc-950 flex flex-col items-center justify-center p-6",
                div {
                    class: "relative w-32 h-32 rounded-full border-4 border-zinc-700 bg-zinc-800 flex items-center justify-center",
                    div {
                        class: "absolute w-2 h-8 bg-amber-500 rounded-full",
                        style: "top: 8px; transform-origin: bottom center; transform: rotate(-45deg);"
                    }
                    div {
                        class: "text-center",
                        div { class: "text-2xl font-bold text-foreground", "+3.2" }
                        div { class: "text-xs text-muted-foreground", "dB" }
                    }
                }
                div { class: "mt-4 text-sm font-medium text-muted-foreground uppercase tracking-wider", "Gain" }
                div {
                    class: "absolute right-8 top-1/2 -translate-y-1/2 flex gap-2",
                    for channel in ["L", "R"] {
                        div {
                            key: "{channel}",
                            class: "flex flex-col items-center gap-1",
                            div {
                                class: "w-4 h-48 bg-zinc-800 rounded overflow-hidden flex flex-col-reverse",
                                div {
                                    class: "w-full bg-gradient-to-t from-green-500 via-yellow-500 to-red-500",
                                    style: "height: 65%;"
                                }
                            }
                            span { class: "text-xs text-muted-foreground", "{channel}" }
                        }
                    }
                }
            }
        }
    }
}

/// P2P Collaboration showcase card
#[component]
fn CollaborationShowcaseCard() -> Element {
    rsx! {
        div {
            class: "rounded-lg border border-border bg-card overflow-hidden shadow-2xl w-full",
            style: "height: 600px;",

            div {
                class: "flex items-center gap-2 px-4 py-3 border-b border-border bg-zinc-900/80",
                div { class: "w-3 h-3 rounded-full bg-red-500/80" }
                div { class: "w-3 h-3 rounded-full bg-yellow-500/80" }
                div { class: "w-3 h-3 rounded-full bg-green-500/80" }
                span { class: "ml-4 text-sm text-muted-foreground", "Session" }
                span { class: "ml-auto text-xs text-cyan-500 font-medium px-2 py-0.5 rounded bg-cyan-500/10", "P2P Sync" }
            }

            div {
                class: "relative h-[calc(100%-3rem)] bg-zinc-900 p-6",
                div {
                    class: "flex items-center justify-center gap-8 mb-8",
                    div {
                        class: "relative w-64 h-64",
                        div {
                            class: "absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-16 h-16 rounded-full bg-primary/20 border-2 border-primary flex items-center justify-center",
                            User { class: "w-8 h-8 text-primary" }
                        }
                        div {
                            class: "absolute left-1/2 top-0 -translate-x-1/2 w-12 h-12 rounded-full bg-emerald-500/20 border-2 border-emerald-500 flex items-center justify-center",
                            User { class: "w-6 h-6 text-emerald-500" }
                        }
                        div {
                            class: "absolute right-0 top-1/2 -translate-y-1/2 w-12 h-12 rounded-full bg-violet-500/20 border-2 border-violet-500 flex items-center justify-center",
                            User { class: "w-6 h-6 text-violet-500" }
                        }
                        div {
                            class: "absolute left-0 top-1/2 -translate-y-1/2 w-12 h-12 rounded-full bg-amber-500/20 border-2 border-amber-500 flex items-center justify-center",
                            User { class: "w-6 h-6 text-amber-500" }
                        }
                        svg {
                            class: "absolute inset-0 w-full h-full",
                            view_box: "0 0 256 256",
                            line { x1: "128", y1: "128", x2: "128", y2: "48", stroke: "currentColor", stroke_opacity: "0.2", stroke_width: "2", stroke_dasharray: "4 4", class: "text-emerald-500" }
                            line { x1: "128", y1: "128", x2: "208", y2: "128", stroke: "currentColor", stroke_opacity: "0.2", stroke_width: "2", stroke_dasharray: "4 4", class: "text-violet-500" }
                            line { x1: "128", y1: "128", x2: "48", y2: "128", stroke: "currentColor", stroke_opacity: "0.2", stroke_width: "2", stroke_dasharray: "4 4", class: "text-amber-500" }
                        }
                    }
                }
                div {
                    class: "absolute bottom-4 left-4 right-4 flex items-center justify-between text-sm",
                    div {
                        class: "flex items-center gap-2 text-emerald-500",
                        div { class: "w-2 h-2 rounded-full bg-emerald-500 animate-pulse" }
                        "3 peers connected"
                    }
                    div { class: "text-muted-foreground", "iroh • encrypted" }
                }
            }
        }
    }
}

// =============================================================================
// Documentation Pages
// =============================================================================

/// Docs home page
#[component]
fn DocsHome() -> Element {
    rsx! {
        div {
            class: "max-w-5xl mx-auto px-6 py-16",
            div {
                class: "text-center mb-16",
                Heading { level: HeadingLevel::H1, class: "text-foreground mb-4", "Documentation" }
                Text { variant: TextVariant::Muted, class: "text-xl", "Everything you need to build with FastTrackStudio" }
            }
            div {
                class: "grid md:grid-cols-2 gap-6",

                DocsSection { to: Route::DocsKeyflow {}, title: "Keyflow", description: "Chart notation language with GPU-accelerated rendering. Write chord charts with intuitive syntax.", icon: rsx! { FileText { class: "w-8 h-8" } }, color: "emerald" }
                DocsSection { to: Route::DocsReaper {}, title: "REAPER Extension", description: "Deep DAW integration for transport control, MIDI routing, and real-time state synchronization.", icon: rsx! { Music { class: "w-8 h-8" } }, color: "violet" }
                DocsSection { to: Route::DocsDesktop {}, title: "Desktop App", description: "Cross-platform application built with Dioxus for setlist management, lyrics, and live performance.", icon: rsx! { fts_ui::lucide_dioxus::Monitor { class: "w-8 h-8" } }, color: "blue" }
                DocsSection { to: Route::DocsPlugins {}, title: "Audio Plugins", description: "CLAP and VST3 plugins built with nih-plug for audio processing and instrument control.", icon: rsx! { fts_ui::lucide_dioxus::SlidersHorizontal { class: "w-8 h-8" } }, color: "amber" }
            }
        }
    }
}

#[component]
fn DocsSection(to: Route, title: &'static str, description: &'static str, icon: Element, color: &'static str) -> Element {
    let (bg_color, border_color, text_color) = match color {
        "emerald" => ("bg-emerald-500/10", "hover:border-emerald-500/50", "text-emerald-500"),
        "violet" => ("bg-violet-500/10", "hover:border-violet-500/50", "text-violet-500"),
        "blue" => ("bg-blue-500/10", "hover:border-blue-500/50", "text-blue-500"),
        "amber" => ("bg-amber-500/10", "hover:border-amber-500/50", "text-amber-500"),
        _ => ("bg-primary/10", "hover:border-primary/50", "text-primary"),
    };

    rsx! {
        Link {
            to: to,
            class: "group block rounded-xl border border-border bg-card p-6 transition-all {border_color} hover:shadow-lg",
            div {
                class: "flex items-start gap-4",
                div { class: "shrink-0 rounded-lg p-3 {bg_color} {text_color}", {icon} }
                div {
                    h3 { class: "text-xl font-semibold text-foreground mb-2 group-hover:text-primary transition-colors", "{title}" }
                    p { class: "text-muted-foreground leading-relaxed", "{description}" }
                    div { class: "mt-4 flex items-center gap-1 text-sm font-medium {text_color}", "Learn more", ChevronRight { class: "w-4 h-4" } }
                }
            }
        }
    }
}

/// Keyflow documentation landing page
#[component]
fn DocsKeyflow() -> Element {
    rsx! {
        div {
            class: "max-w-5xl mx-auto px-6 py-12",
            Link { to: Route::DocsHome {}, class: "inline-flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground mb-8 transition-colors", ArrowLeft { class: "w-4 h-4" }, "Back to Docs" }

            div {
                class: "mb-12",
                div { class: "inline-flex items-center gap-3 mb-4",
                    div { class: "rounded-lg p-3 bg-emerald-500/10 text-emerald-500", FileText { class: "w-8 h-8" } }
                    Heading { level: HeadingLevel::H1, class: "text-foreground", "Keyflow" }
                }
                Text { variant: TextVariant::Muted, class: "text-xl max-w-2xl", "A domain-specific language for writing chord charts with GPU-accelerated rendering." }
            }

            div {
                class: "grid md:grid-cols-2 gap-6 mb-12",
                Link { to: Route::SnippetsBrowser {}, class: "group flex items-center gap-4 p-6 rounded-xl border border-border bg-card hover:border-emerald-500/50 transition-all",
                    div { class: "rounded-lg p-3 bg-emerald-500/10 text-emerald-500", FileCode { class: "w-6 h-6" } }
                    div {
                        h3 { class: "font-semibold text-foreground group-hover:text-emerald-500 transition-colors", "Interactive Snippets" }
                        p { class: "text-sm text-muted-foreground", "Browse and edit example charts" }
                    }
                    ChevronRight { class: "w-5 h-5 text-muted-foreground ml-auto" }
                }
                Link { to: Route::ChartEditor {}, class: "group flex items-center gap-4 p-6 rounded-xl border border-border bg-card hover:border-emerald-500/50 transition-all",
                    div { class: "rounded-lg p-3 bg-emerald-500/10 text-emerald-500", PenLine { class: "w-6 h-6" } }
                    div {
                        h3 { class: "font-semibold text-foreground group-hover:text-emerald-500 transition-colors", "Chart Editor" }
                        p { class: "text-sm text-muted-foreground", "Write and export your own charts" }
                    }
                    ChevronRight { class: "w-5 h-5 text-muted-foreground ml-auto" }
                }
            }

            div {
                class: "prose prose-invert max-w-none",
                Heading { level: HeadingLevel::H3, class: "text-foreground mb-6", "Features" }
                div {
                    class: "grid md:grid-cols-3 gap-6",
                    KeyflowFeatureCard { title: "Intuitive Syntax", description: "Write chord charts as naturally as you'd read them. No complex markup required." }
                    KeyflowFeatureCard { title: "Smart Memory", description: "Automatically remembers chord voicings within sections for consistent playback." }
                    KeyflowFeatureCard { title: "Section Structure", description: "Verse, Chorus, Bridge, and more - with automatic numbering and theming." }
                    KeyflowFeatureCard { title: "Complex Rhythms", description: "Triplet pushes, syncopation, and explicit durations for any rhythmic pattern." }
                    KeyflowFeatureCard { title: "GPU Rendering", description: "WebGPU-accelerated rendering for crisp, publication-quality output." }
                    KeyflowFeatureCard { title: "PDF Export", description: "Export charts as print-ready PDFs with customizable settings." }
                }
            }
        }
    }
}

#[component]
fn KeyflowFeatureCard(title: &'static str, description: &'static str) -> Element {
    rsx! {
        Card { class: "border-border/50 bg-card/30",
            CardContent { class: "p-4",
                CardTitle { class: "text-sm mb-1", "{title}" }
                CardDescription { "{description}" }
            }
        }
    }
}

/// REAPER Extension documentation page
#[component]
fn DocsReaper() -> Element {
    rsx! {
        div {
            class: "max-w-5xl mx-auto px-6 py-12",
            Link { to: Route::DocsHome {}, class: "inline-flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground mb-8 transition-colors", ArrowLeft { class: "w-4 h-4" }, "Back to Docs" }
            div { class: "mb-12",
                div { class: "inline-flex items-center gap-3 mb-4",
                    div { class: "rounded-lg p-3 bg-violet-500/10 text-violet-500", Music { class: "w-8 h-8" } }
                    Heading { level: HeadingLevel::H1, class: "text-foreground", "REAPER Extension" }
                }
                Text { variant: TextVariant::Muted, class: "text-xl max-w-2xl", "Deep DAW integration for transport control, MIDI routing, and real-time state synchronization." }
            }
            div { class: "space-y-6",
                ReaperFeatureSection { title: "Transport Sync", description: "Real-time synchronization with REAPER's transport for play/pause/stop controls, timeline position, and tempo changes." }
                ReaperFeatureSection { title: "MIDI Routing", description: "Flexible MIDI routing between tracks, virtual instruments, and external hardware." }
                ReaperFeatureSection { title: "Key Input", description: "Intercept and redirect keyboard input for custom shortcuts and live performance controls." }
                ReaperFeatureSection { title: "IPC Communication", description: "Bidirectional communication with the desktop app via iroh for seamless integration." }
            }
            Card { class: "mt-12 p-6 border-violet-500/30 bg-violet-500/5",
                CardTitle { class: "mb-2", "Documentation Coming Soon" }
                CardDescription { "Detailed installation guides, API reference, and usage examples are being written." }
            }
        }
    }
}

#[component]
fn ReaperFeatureSection(title: &'static str, description: &'static str) -> Element {
    rsx! {
        Card { class: "p-6",
            CardTitle { class: "mb-2", "{title}" }
            CardDescription { "{description}" }
        }
    }
}

/// Desktop App documentation page
#[component]
fn DocsDesktop() -> Element {
    rsx! {
        div {
            class: "max-w-5xl mx-auto px-6 py-12",
            Link { to: Route::DocsHome {}, class: "inline-flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground mb-8 transition-colors", ArrowLeft { class: "w-4 h-4" }, "Back to Docs" }
            div { class: "mb-12",
                div { class: "inline-flex items-center gap-3 mb-4",
                    div { class: "rounded-lg p-3 bg-blue-500/10 text-blue-500", fts_ui::lucide_dioxus::Monitor { class: "w-8 h-8" } }
                    Heading { level: HeadingLevel::H1, class: "text-foreground", "Desktop App" }
                }
                Text { variant: TextVariant::Muted, class: "text-xl max-w-2xl", "Cross-platform application for setlist management, lyrics display, and live performance." }
            }
            div { class: "grid md:grid-cols-2 gap-6",
                DesktopFeatureCard { title: "Setlist Management", description: "Organize songs into setlists with drag-and-drop ordering and quick access." }
                DesktopFeatureCard { title: "Lyrics Display", description: "Full-screen lyrics view with auto-scroll synced to transport position." }
                DesktopFeatureCard { title: "Chart Viewer", description: "View and edit Keyflow charts with real-time preview and PDF export." }
                DesktopFeatureCard { title: "P2P Sync", description: "Share setlists and charts with bandmates in real-time over peer-to-peer connections." }
            }
            Card { class: "mt-12 p-6 border-blue-500/30 bg-blue-500/5",
                CardTitle { class: "mb-2", "Documentation Coming Soon" }
                CardDescription { "Installation guides, feature walkthroughs, and configuration options are being written." }
            }
        }
    }
}

#[component]
fn DesktopFeatureCard(title: &'static str, description: &'static str) -> Element {
    rsx! {
        Card { class: "p-6",
            CardTitle { class: "mb-2", "{title}" }
            CardDescription { "{description}" }
        }
    }
}

/// Audio Plugins documentation page
#[component]
fn DocsPlugins() -> Element {
    rsx! {
        div {
            class: "max-w-5xl mx-auto px-6 py-12",
            Link { to: Route::DocsHome {}, class: "inline-flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground mb-8 transition-colors", ArrowLeft { class: "w-4 h-4" }, "Back to Docs" }
            div { class: "mb-12",
                div { class: "inline-flex items-center gap-3 mb-4",
                    div { class: "rounded-lg p-3 bg-amber-500/10 text-amber-500", fts_ui::lucide_dioxus::SlidersHorizontal { class: "w-8 h-8" } }
                    Heading { level: HeadingLevel::H1, class: "text-foreground", "Audio Plugins" }
                }
                Text { variant: TextVariant::Muted, class: "text-xl max-w-2xl", "CLAP and VST3 plugins built with nih-plug for audio processing and instrument control." }
            }
            div { class: "space-y-6",
                PluginCard { name: "fts-guide", description: "Click track, count-in, and section guide for live performance navigation." }
                PluginCard { name: "fts-macros", description: "Macro parameter surface with 8 automatable parameters for REAPER integration." }
            }
            Card { class: "mt-12 p-6 border-amber-500/30 bg-amber-500/5",
                CardTitle { class: "mb-2", "Built with nih-plug" }
                CardDescription { class: "mb-4", "All plugins are built using nih-plug, a Rust framework for creating CLAP and VST3 plugins with minimal boilerplate." }
                a {
                    href: "https://github.com/robbert-vdh/nih-plug",
                    target: "_blank",
                    class: "inline-flex items-center gap-2 text-amber-500 hover:text-amber-400 transition-colors",
                    "Learn more about nih-plug"
                    ExternalLink { class: "w-4 h-4" }
                }
            }
        }
    }
}

#[component]
fn PluginCard(name: &'static str, description: &'static str) -> Element {
    rsx! {
        Card { class: "p-6",
            div { class: "flex items-center gap-3 mb-2",
                Badge { class: "border-amber-500/30 bg-amber-500/10 text-amber-500 font-mono", "{name}" }
            }
            CardDescription { "{description}" }
        }
    }
}

#[component]
fn FeatureCard(title: &'static str, description: &'static str, icon: Element) -> Element {
    rsx! {
        Card {
            class: "p-6 hover:border-primary/50 transition-colors",
            div { class: "mb-4", {icon} }
            Heading { level: HeadingLevel::H5, class: "text-card-foreground mb-2", "{title}" }
            CardDescription { "{description}" }
        }
    }
}

// =============================================================================
// Snippets Browser
// =============================================================================

/// Unified snippets browser
#[component]
fn SnippetsBrowser() -> Element {
    let patterns = keyflow::patterns::all_patterns();
    let first_id = patterns.first().map(|p| p.id).unwrap_or("minimal-chart");
    rsx! { UnifiedSnippetsView { selected_id: first_id.to_string() } }
}

#[component]
fn SnippetsView(id: String) -> Element {
    rsx! { UnifiedSnippetsView { selected_id: id } }
}

#[component]
fn UnifiedSnippetsView(selected_id: String) -> Element {
    use components::PreviewMode;
    use keyflow::patterns::{PatternCategory, all_patterns, find_pattern, patterns_by_category};

    let patterns = all_patterns();
    let pattern = find_pattern(&selected_id);

    let mut expanded_categories = use_signal(|| {
        PatternCategory::all().iter().map(|c| (*c, true)).collect::<std::collections::HashMap<_, _>>()
    });

    let current_index = patterns.iter().position(|p| p.id == selected_id).unwrap_or(0);
    let prev_pattern = if current_index > 0 { patterns.get(current_index - 1) } else { None };
    let next_pattern = patterns.get(current_index + 1);

    let mut last_pattern_id = use_signal(|| selected_id.clone());
    let mut source = use_signal(|| pattern.map(|p| p.source.to_string()).unwrap_or_default());
    let mut preview_mode = use_signal(|| {
        pattern.map(|p| if p.category == PatternCategory::Examples { PreviewMode::Page } else { PreviewMode::Snippet }).unwrap_or(PreviewMode::Snippet)
    });

    if *last_pattern_id.peek() != selected_id {
        if let Some(p) = pattern {
            source.set(p.source.to_string());
            let new_mode = if p.category == PatternCategory::Examples { PreviewMode::Page } else { PreviewMode::Snippet };
            preview_mode.set(new_mode);
        }
        last_pattern_id.set(selected_id.clone());
    }

    let mut show_source = use_signal(|| true);

    rsx! {
        div {
            class: "flex h-[calc(100vh-4rem)]",

            // Sidebar
            aside {
                class: "w-72 bg-sidebar border-r border-sidebar-border flex flex-col",
                div { class: "p-4 border-b border-sidebar-border",
                    h2 { class: "text-lg font-semibold text-sidebar-foreground", "Snippets" }
                    p { class: "text-xs text-muted-foreground mt-1", "{patterns.len()} interactive examples" }
                }
                nav {
                    class: "flex-1 overflow-y-auto p-2",
                    for category in PatternCategory::all() {
                        {
                            let category = *category;
                            let cat_patterns = patterns_by_category(category);
                            let is_expanded = expanded_categories.read().get(&category).copied().unwrap_or(true);
                            let has_selected = cat_patterns.iter().any(|p| p.id == selected_id);
                            rsx! {
                                div { class: "mb-2",
                                    button {
                                        class: "w-full flex items-center justify-between px-3 py-2 rounded-md text-sm font-medium text-sidebar-foreground hover:bg-sidebar-accent transition-colors",
                                        onclick: move |_| {
                                            let mut cats = expanded_categories.write();
                                            let current = cats.get(&category).copied().unwrap_or(true);
                                            cats.insert(category, !current);
                                        },
                                        span { class: if has_selected { "text-primary" } else { "" }, "{category.label()}" }
                                        span { class: "text-xs text-muted-foreground",
                                            if is_expanded {
                                                ChevronDown { class: "w-4 h-4" }
                                            } else {
                                                ChevronRight { class: "w-4 h-4" }
                                            }
                                        }
                                    }
                                    if is_expanded {
                                        div { class: "ml-2 mt-1 space-y-0.5",
                                            for pattern in cat_patterns {
                                                {
                                                    let is_selected = pattern.id == selected_id;
                                                    rsx! {
                                                        Link {
                                                            to: Route::SnippetsView { id: pattern.id.to_string() },
                                                            class: if is_selected {
                                                                "block px-3 py-1.5 rounded-md text-sm bg-primary/10 text-primary font-medium border-l-2 border-primary"
                                                            } else {
                                                                "block px-3 py-1.5 rounded-md text-sm text-sidebar-foreground hover:bg-sidebar-accent transition-colors border-l-2 border-transparent"
                                                            },
                                                            "{pattern.title}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "p-4 border-t border-sidebar-border",
                    Link { to: Route::ChartEditor {}, class: "flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors",
                        PenLine { class: "w-4 h-4" }
                        "Open Full Editor"
                    }
                }
            }

            // Main content
            div {
                class: "flex-1 flex flex-col min-w-0",
                match pattern {
                    Some(pattern) => rsx! {
                        header {
                            class: "px-6 py-4 border-b border-border flex items-center justify-between shrink-0 bg-card/50",
                            div { class: "min-w-0",
                                div { class: "flex items-center gap-3",
                                    span { class: "text-xs font-medium text-primary bg-primary/10 px-2 py-0.5 rounded", "{pattern.category.label()}" }
                                    h1 { class: "text-xl font-semibold text-foreground truncate", "{pattern.title}" }
                                }
                                p { class: "text-sm text-muted-foreground mt-1 line-clamp-1", "{pattern.description}" }
                            }
                            div { class: "flex items-center gap-2 shrink-0 ml-4",
                                div { class: "flex items-center gap-1",
                                    if let Some(prev) = prev_pattern {
                                        Link { to: Route::SnippetsView { id: prev.id.to_string() }, class: "p-2 rounded-md hover:bg-accent text-muted-foreground hover:text-foreground transition-colors", title: "Previous: {prev.title}", ChevronLeft { class: "w-4 h-4" } }
                                    } else {
                                        span { class: "p-2 text-muted-foreground/30", ChevronLeft { class: "w-4 h-4" } }
                                    }
                                    span { class: "text-xs text-muted-foreground px-2", "{current_index + 1} / {patterns.len()}" }
                                    if let Some(next) = next_pattern {
                                        Link { to: Route::SnippetsView { id: next.id.to_string() }, class: "p-2 rounded-md hover:bg-accent text-muted-foreground hover:text-foreground transition-colors", title: "Next: {next.title}", ChevronRight { class: "w-4 h-4" } }
                                    } else {
                                        span { class: "p-2 text-muted-foreground/30", ChevronRight { class: "w-4 h-4" } }
                                    }
                                }
                                div { class: "w-px h-6 bg-border mx-2" }
                                button {
                                    class: if *show_source.read() {
                                        "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-primary/10 text-primary"
                                    } else {
                                        "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium text-muted-foreground hover:bg-accent transition-colors"
                                    },
                                    onclick: move |_| { let current = *show_source.peek(); show_source.set(!current); },
                                    Code { class: "w-3.5 h-3.5" }
                                    "Source"
                                }
                            }
                        }
                        div {
                            class: "flex-1 flex overflow-hidden",
                            div { class: "flex-1 overflow-hidden bg-muted/30",
                                components::DynamicChartRenderer { source: source, mode: preview_mode, canvas_id: Some(format!("snippet-canvas-{}", pattern.id)) }
                            }
                            if *show_source.read() {
                                div { class: "w-96 border-l border-border flex flex-col bg-card shrink-0",
                                    div { class: "px-4 py-3 border-b border-border flex items-center justify-between shrink-0",
                                        div { class: "flex items-center gap-2",
                                            FileCode { class: "w-4 h-4 text-muted-foreground" }
                                            span { class: "text-sm font-medium text-foreground", "Source" }
                                        }
                                        if source.read().as_str() != pattern.source {
                                            button { class: "text-xs text-muted-foreground hover:text-foreground px-2 py-1 rounded hover:bg-accent transition-colors", onclick: move |_| source.set(pattern.source.to_string()), "Reset" }
                                        }
                                    }
                                    div { class: "flex-1 overflow-hidden",
                                        components::HighlightedEditor { value: source(), on_change: move |v: String| source.set(v), placeholder: "Enter keyflow notation...", textarea_id: Some(format!("snippet-editor-{}", pattern.id)) }
                                    }
                                }
                            }
                        }
                    },
                    None => {
                        rsx! {
                            div { class: "flex-1 flex flex-col items-center justify-center text-center p-8",
                                fts_ui::lucide_dioxus::CircleAlert { class: "w-16 h-16 text-muted-foreground/50 mb-4" }
                                h2 { class: "text-xl font-semibold text-foreground mb-2", "Pattern Not Found" }
                                p { class: "text-muted-foreground mb-6", "The pattern \"{selected_id}\" doesn't exist." }
                                Link { to: Route::SnippetsBrowser {}, class: "inline-flex items-center gap-2 px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors", "Browse All Snippets" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Legacy Routes
// =============================================================================

#[component]
fn PatternBrowser() -> Element {
    let nav = use_navigator();
    use_effect(move || { nav.push(Route::SnippetsBrowser {}); });
    rsx! { div { class: "flex items-center justify-center h-64 text-muted-foreground", "Redirecting..." } }
}

#[component]
fn ChartEditor() -> Element {
    rsx! { components::ChartEditor {} }
}

#[component]
fn PatternView(id: String) -> Element {
    let nav = use_navigator();
    let id_clone = id.clone();
    use_effect(move || { nav.push(Route::SnippetsView { id: id_clone.clone() }); });
    rsx! { div { class: "flex items-center justify-center h-64 text-muted-foreground", "Redirecting..." } }
}

// =============================================================================
// Test Pages
// =============================================================================

/// Test page for debugging chart rendering
#[component]
fn TestRender() -> Element {
    let mut source = use_signal(|| DEMO_CHARTS[0].to_string());
    let preview_mode = use_signal(|| components::PreviewMode::Page);

    rsx! {
        div {
            class: "flex h-[calc(100vh-8rem)]",
            div {
                class: "w-96 border-r border-border overflow-hidden",
                div { class: "px-4 py-3 border-b border-border bg-card",
                    h3 { class: "text-sm font-semibold", "Source Code" }
                }
                div { class: "h-[calc(100%-3rem)]",
                    components::HighlightedEditor { value: source(), on_change: move |new_value: String| source.set(new_value), placeholder: "Enter keyflow chart notation...", textarea_id: Some("test-render-editor".to_string()) }
                }
            }
            div {
                class: "flex-1 bg-zinc-800 overflow-auto p-8",
                div { class: "inline-block", style: "border: 1px solid red;",
                    components::StaticChartRenderer { source: source, mode: preview_mode, canvas_id: Some("test-render-canvas".to_string()) }
                }
            }
        }
    }
}

/// Combined test page for EQ and Compressor graph components
#[component]
fn TestFxUi() -> Element {
    // === EQ State ===
    let mut bands = use_signal(|| {
        vec![
            EqBand { index: 0, used: true, enabled: true, frequency: 80.0, gain: 3.0, q: 0.7, shape: EqBandShape::LowShelf, ..Default::default() },
            EqBand { index: 1, used: true, enabled: true, frequency: 250.0, gain: -2.5, q: 1.5, shape: EqBandShape::Bell, ..Default::default() },
            EqBand { index: 2, used: true, enabled: true, frequency: 1000.0, gain: 1.5, q: 2.0, shape: EqBandShape::Bell, ..Default::default() },
            EqBand { index: 3, used: true, enabled: true, frequency: 4000.0, gain: 2.0, q: 1.0, shape: EqBandShape::Bell, ..Default::default() },
            EqBand { index: 4, used: true, enabled: true, frequency: 12000.0, gain: 4.0, q: 0.8, shape: EqBandShape::HighShelf, ..Default::default() },
        ]
    });
    let mut selected_band = use_signal(|| 0_usize);
    let mut eq_db_range = use_signal(|| 24.0_f64);

    // === Compressor State ===
    let mut params = use_signal(CompressorParams::default);
    let mut metering = use_signal(CompressorMetering::default);
    let mut sim_time = use_signal(|| 0.0_f64);
    let mut comp_db_range = use_signal(|| DbRange::Range48);

    // Animation effect for simulated compressor levels
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::prelude::*;

            let closure = Closure::wrap(Box::new(move || {
                let t = *sim_time.peek();
                sim_time.set(t + 0.05);

                let input = -24.0 + 18.0 * (t * 0.7).sin() as f32 + 6.0 * (t * 2.3).sin() as f32;
                let input_clamped = input.clamp(-60.0, 0.0);

                let p = params.peek();
                let threshold = p.threshold;
                let ratio = p.ratio;
                let knee_w = p.knee / 2.0;

                let output = if input_clamped <= threshold - knee_w {
                    input_clamped
                } else if input_clamped < threshold + knee_w {
                    let a0 = (1.0 / ratio - 1.0) / (4.0 * knee_w);
                    let x_offset = input_clamped - (threshold - knee_w);
                    input_clamped + a0 * x_offset * x_offset
                } else {
                    threshold + (input_clamped - threshold) / ratio
                };

                let gr = output - input_clamped;

                let prev = metering.peek();
                let input_peak = input_clamped.max(prev.input_peak * 0.99);
                let output_peak = output.max(prev.output_peak * 0.99);
                let gr_peak = gr.min(prev.gr_peak * 0.99);
                let mut gr_history = prev.gr_history.clone();
                let mut input_history = prev.input_history.clone();
                drop(prev);

                gr_history.push(gr);
                input_history.push(input_clamped);

                const MAX_HISTORY: usize = 128;
                if gr_history.len() > MAX_HISTORY { gr_history.drain(0..gr_history.len() - MAX_HISTORY); }
                if input_history.len() > MAX_HISTORY { input_history.drain(0..input_history.len() - MAX_HISTORY); }

                metering.set(CompressorMetering {
                    input_level: input_clamped,
                    output_level: output,
                    gain_reduction: gr,
                    input_peak,
                    output_peak,
                    gr_peak,
                    gr_history,
                    input_history,
                });
            }) as Box<dyn FnMut()>);

            let window = web_sys::window().unwrap();
            let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(closure.as_ref().unchecked_ref(), 50);
            closure.forget();
        }
    });

    rsx! {
        div {
            class: "min-h-screen bg-background p-6",
            div { class: "max-w-7xl mx-auto mb-6",
                h1 { class: "text-2xl font-bold text-foreground mb-2", "Audio FX Controls" }
                p { class: "text-muted-foreground text-sm", "Interactive EQ and Compressor widgets for audio production interfaces." }
            }
            div {
                class: "max-w-7xl mx-auto grid grid-cols-1 xl:grid-cols-2 gap-6",

                // EQ Section
                div { class: "bg-card rounded-xl border border-border p-4",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-lg font-semibold text-foreground", "Parametric EQ" }
                        div { class: "flex items-center gap-2",
                            label { class: "text-xs text-muted-foreground", "Range:" }
                            select {
                                class: "px-2 py-1 rounded bg-muted border border-border text-foreground text-xs",
                                value: "{*eq_db_range.read()}",
                                onchange: move |evt: Event<FormData>| { if let Ok(v) = evt.value().parse::<f64>() { eq_db_range.set(v); } },
                                option { value: "6", "±6 dB" }
                                option { value: "12", "±12 dB" }
                                option { value: "18", "±18 dB" }
                                option { value: "24", "±24 dB" }
                                option { value: "30", "±30 dB" }
                            }
                        }
                    }
                    div { class: "w-full mb-4", style: "aspect-ratio: 800 / 350;",
                        EqGraph {
                            bands: bands,
                            db_range: *eq_db_range.read(),
                            show_grid: true,
                            fill_curve: true,
                            on_band_change: move |(idx, band): (usize, EqBand)| { let mut b = bands.write(); if idx < b.len() { b[idx] = band; } },
                            on_band_add: move |band: EqBand| { bands.write().push(band); },
                            on_band_remove: move |idx: usize| { let mut b = bands.write(); if idx < b.len() { b.remove(idx); let sel_idx = *selected_band.peek(); if sel_idx >= b.len() && !b.is_empty() { selected_band.set(b.len() - 1); } } },
                            on_begin: move |idx: usize| { selected_band.set(idx); },
                        }
                    }
                    div { class: "flex flex-wrap gap-2 text-xs",
                        for (i, band) in bands.read().iter().enumerate() {
                            div {
                                class: if i == *selected_band.read() { "px-2 py-1 rounded bg-primary/20 border border-primary/40 text-foreground" } else { "px-2 py-1 rounded bg-muted border border-border text-muted-foreground" },
                                onclick: move |_| selected_band.set(i),
                                "{format_frequency(band.frequency)} • {band.gain:+.1}dB"
                            }
                        }
                    }
                }

                // Compressor Section
                div { class: "bg-card rounded-xl border border-border p-4",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-lg font-semibold text-foreground", "Compressor" }
                        span { class: "text-xs text-muted-foreground", "Pro-C Style" }
                    }
                    div { class: "flex justify-center",
                        CompressorWidget {
                            params: params,
                            metering: metering.read().clone(),
                            db_range: *comp_db_range.read(),
                            graph_size: 200,
                            show_grid: true,
                            show_gr_meter: true,
                            show_levels: true,
                            show_gr_trace: true,
                            show_controls: true,
                            interactive: true,
                        }
                    }
                }
            }

            // Gate Section
            GateSection {}

            // Instructions
            div {
                class: "max-w-7xl mx-auto mt-6 p-4 bg-muted/50 rounded-lg border border-border",
                h3 { class: "text-sm font-semibold text-foreground mb-2", "Interaction Guide" }
                div { class: "grid grid-cols-1 md:grid-cols-3 gap-4 text-xs text-muted-foreground",
                    div {
                        h4 { class: "font-medium text-foreground mb-1", "EQ" }
                        ul { class: "space-y-0.5 list-disc list-inside",
                            li { "Drag bands to adjust frequency/gain" }
                            li { "Mouse wheel to adjust Q" }
                            li { "Double-click to add new band" }
                            li { "Drag outside to remove band" }
                            li { "Shift+drag for fine control" }
                        }
                    }
                    div {
                        h4 { class: "font-medium text-foreground mb-1", "Compressor" }
                        ul { class: "space-y-0.5 list-disc list-inside",
                            li { "Drag threshold point to adjust threshold" }
                            li { "Drag above threshold to adjust ratio" }
                            li { "Mouse wheel to adjust knee width" }
                            li { "Shift+drag for fine control" }
                        }
                    }
                    div {
                        h4 { class: "font-medium text-foreground mb-1", "Gate" }
                        ul { class: "space-y-0.5 list-disc list-inside",
                            li { "Use knobs to adjust parameters" }
                            li { "Large knob controls threshold" }
                            li { "Small knobs for ratio/range" }
                            li { "Right panel for envelope controls" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GateSection() -> Element {
    let gate_params = use_signal(GateParams::default);
    rsx! {
        div { class: "max-w-7xl mx-auto mt-6 bg-card rounded-xl border border-border p-4",
            div { class: "flex items-center justify-between mb-4",
                h2 { class: "text-lg font-semibold text-foreground", "Noise Gate" }
                span { class: "text-xs text-muted-foreground", "Pro-G Style" }
            }
            div { class: "flex justify-center",
                GateGraph {
                    params: gate_params,
                    metering: GateMetering::default(),
                    db_range: GateDbRange::Range60,
                    graph_size: 200,
                    show_grid: true,
                    show_gr_meter: true,
                    show_levels: false,
                    show_gr_trace: false,
                    show_controls: true,
                    interactive: true,
                }
            }
        }
    }
}

fn format_frequency(freq: f32) -> String {
    if freq >= 1000.0 { format!("{:.1}k", freq / 1000.0) } else { format!("{:.0}", freq) }
}
