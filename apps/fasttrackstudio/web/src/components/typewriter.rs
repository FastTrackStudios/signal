//! Typewriter animation component.
//!
//! Displays text with a typing animation effect, cycling through multiple phrases.

use dioxus::prelude::*;

/// Props for the Typewriter component.
#[derive(Props, Clone, PartialEq)]
pub struct TypewriterProps {
    /// List of text phrases to cycle through.
    pub words: Vec<String>,
    /// Typing speed in milliseconds per character.
    #[props(default = 80)]
    pub speed_ms: u64,
    /// Delay between finishing one word and starting to delete (ms).
    #[props(default = 2000)]
    pub delay_between_words_ms: u64,
    /// Whether to show a blinking cursor.
    #[props(default = true)]
    pub cursor: bool,
    /// Character to use for the cursor.
    #[props(default = "|".to_string())]
    pub cursor_char: String,
}

/// Typewriter animation component.
///
/// Displays text character by character, cycling through a list of phrases
/// with typing and deleting animations.
#[component]
pub fn Typewriter(props: TypewriterProps) -> Element {
    let mut display_text = use_signal(String::new);
    let mut is_deleting = use_signal(|| false);
    let mut word_index = use_signal(|| 0_usize);
    let mut char_index = use_signal(|| 0_usize);
    let mut show_cursor = use_signal(|| true);
    let mut is_paused = use_signal(|| false);

    let words = props.words.clone();
    let speed_ms = props.speed_ms;
    let delay_between_words_ms = props.delay_between_words_ms;

    // Main typing effect
    use_effect(move || {
        let words = words.clone();

        spawn(async move {
            loop {
                // Wait for appropriate delay
                let delay = if *is_paused.read() {
                    // Paused at end of word, wait for delay
                    is_paused.set(false);
                    delay_between_words_ms
                } else if *is_deleting.read() {
                    speed_ms / 2 // Delete faster
                } else {
                    speed_ms
                };

                #[cfg(target_arch = "wasm32")]
                {
                    let promise = js_sys::Promise::new(&mut |resolve, _| {
                        if let Some(window) = web_sys::window() {
                            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                                &resolve,
                                delay as i32,
                            );
                        }
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }

                let current_word_idx = *word_index.read();
                if current_word_idx >= words.len() {
                    word_index.set(0);
                    continue;
                }
                let current_word = &words[current_word_idx];
                let current_char_idx = *char_index.read();
                let deleting = *is_deleting.read();

                if !deleting {
                    // Typing
                    if current_char_idx < current_word.len() {
                        display_text.set(current_word[..current_char_idx + 1].to_string());
                        char_index.set(current_char_idx + 1);
                    } else {
                        // Word complete, pause then start deleting
                        is_paused.set(true);
                        is_deleting.set(true);
                    }
                } else {
                    // Deleting
                    if current_char_idx > 0 {
                        display_text.set(current_word[..current_char_idx - 1].to_string());
                        char_index.set(current_char_idx - 1);
                    } else {
                        // Word deleted, move to next
                        is_deleting.set(false);
                        word_index.set((current_word_idx + 1) % words.len());
                    }
                }
            }
        });
    });

    // Cursor blinking effect
    let cursor_enabled = props.cursor;
    use_effect(move || {
        if !cursor_enabled {
            return;
        }

        spawn(async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    let promise = js_sys::Promise::new(&mut |resolve, _| {
                        if let Some(window) = web_sys::window() {
                            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                                &resolve, 530,
                            );
                        }
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    tokio::time::sleep(std::time::Duration::from_millis(530)).await;
                }

                let current = *show_cursor.read();
                show_cursor.set(!current);
            }
        });
    });

    let cursor_opacity = if *show_cursor.read() { "1" } else { "0" };

    rsx! {
        span {
            class: "inline",
            "{display_text}"
            if props.cursor {
                span {
                    class: "ml-0.5 transition-opacity duration-75",
                    style: "opacity: {cursor_opacity};",
                    "{props.cursor_char}"
                }
            }
        }
    }
}

/// Props for the ChartTypewriter component (specialized for chart source code).
#[derive(Props, Clone, PartialEq)]
pub struct ChartTypewriterProps {
    /// Signal to write the current text to (for live chart rendering).
    pub output: Signal<String>,
    /// List of chart source snippets to cycle through.
    pub charts: Vec<String>,
    /// Typing speed in milliseconds per character.
    #[props(default = 50)]
    pub speed_ms: u64,
    /// Delay between finishing one chart and starting to delete (ms).
    #[props(default = 3000)]
    pub delay_between_charts_ms: u64,
}

/// Chart-specific typewriter that updates a signal for live chart rendering.
///
/// Instead of displaying text, this component updates an output signal
/// that can be used to drive a live chart renderer.
#[component]
pub fn ChartTypewriter(props: ChartTypewriterProps) -> Element {
    let charts = props.charts.clone();

    // Start with the first chart fully typed, paused, about to delete
    let first_chart_len = charts.first().map(|c| c.chars().count()).unwrap_or(0);
    let mut is_deleting = use_signal(|| true);
    let mut chart_index = use_signal(|| 0_usize);
    let mut char_index = use_signal(move || first_chart_len);
    let mut is_paused = use_signal(|| true);

    let speed_ms = props.speed_ms;
    let delay_between_charts_ms = props.delay_between_charts_ms;
    let mut output = props.output;

    // Initialize with the first chart fully rendered so content is visible on load
    let init_charts = charts.clone();
    use_effect(move || {
        if let Some(first) = init_charts.first() {
            output.set(first.clone());
        }
    });

    // Main typing effect
    use_effect(move || {
        let charts = charts.clone();

        spawn(async move {
            loop {
                // Wait for appropriate delay
                let delay = if *is_paused.read() {
                    is_paused.set(false);
                    delay_between_charts_ms
                } else if *is_deleting.read() {
                    speed_ms / 3 // Delete faster
                } else {
                    speed_ms
                };

                #[cfg(target_arch = "wasm32")]
                {
                    let promise = js_sys::Promise::new(&mut |resolve, _| {
                        if let Some(window) = web_sys::window() {
                            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                                &resolve,
                                delay as i32,
                            );
                        }
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }

                let current_chart_idx = *chart_index.read();
                if current_chart_idx >= charts.len() {
                    chart_index.set(0);
                    continue;
                }
                let current_chart = &charts[current_chart_idx];
                let current_char_idx = *char_index.read();
                let deleting = *is_deleting.read();

                if !deleting {
                    // Typing
                    if current_char_idx < current_chart.len() {
                        // Get the substring up to current char (handle UTF-8 properly)
                        let text: String =
                            current_chart.chars().take(current_char_idx + 1).collect();
                        output.set(text);
                        char_index.set(current_char_idx + 1);
                    } else {
                        // Chart complete, pause then start deleting
                        is_paused.set(true);
                        is_deleting.set(true);
                    }
                } else {
                    // Deleting
                    if current_char_idx > 0 {
                        let text: String =
                            current_chart.chars().take(current_char_idx - 1).collect();
                        output.set(text);
                        char_index.set(current_char_idx - 1);
                    } else {
                        // Chart deleted, move to next
                        is_deleting.set(false);
                        chart_index.set((current_chart_idx + 1) % charts.len());
                    }
                }
            }
        });
    });

    // This component doesn't render anything visible - it just drives the output signal
    rsx! {}
}
