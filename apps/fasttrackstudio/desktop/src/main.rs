use std::sync::{Arc, Mutex};
use std::{io::Write, path::PathBuf};

use dioxus::desktop::{tao::window::WindowBuilder, Config};
use dioxus::prelude::*;
use keyflow_ui::{ChartGraphics, ChartLayoutManager, ChartView, CHART_SOURCE};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    let config = Config::new()
        .with_window(
            WindowBuilder::new()
                .with_title("FastTrackStudio - Keyflow")
                .with_transparent(true),
        )
        .with_background_color((0, 0, 0, 0))
        .with_on_window(|window, dom| {
            let size = window.inner_size();
            let graphics = ChartGraphics::new(window, size.width, size.height);
            dom.provide_root_context(Arc::new(Mutex::new(graphics)));
        })
        .with_as_child_window();

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}

fn create_svg_zip(pages: &[String]) -> Result<Vec<u8>, String> {
    let mut writer = std::io::Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut writer);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6));

    for (index, svg) in pages.iter().enumerate() {
        let filename = format!("chart-export-page-{}.svg", index + 1);
        zip.start_file(filename, options)
            .map_err(|e| format!("Failed to create zip entry: {e}"))?;
        zip.write_all(svg.as_bytes())
            .map_err(|e| format!("Failed to write SVG entry: {e}"))?;
    }

    zip.finish()
        .map_err(|e| format!("Failed to finalize ZIP: {e}"))?;
    Ok(writer.into_inner())
}

#[component]
fn App() -> Element {
    let mut is_exporting = use_signal(|| false);
    let mut show_export_menu = use_signal(|| false);
    let mut export_status = use_signal(String::new);

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        div {
            class: "h-screen w-screen flex flex-col bg-background text-foreground",
            onclick: move |_| {
                if *show_export_menu.read() {
                    show_export_menu.set(false);
                }
            },

            div {
                class: "h-12 flex-shrink-0 border-b border-border bg-card px-4 flex items-center justify-between",
                div {
                    class: "flex items-center gap-3",
                    h1 { class: "text-sm font-semibold", "Keyflow Editor" }
                    if !export_status.read().is_empty() {
                        span { class: "text-xs text-muted-foreground", "{export_status}" }
                    }
                }

                div {
                    class: "relative",
                    onclick: move |evt| evt.stop_propagation(),
                    div {
                        class: "flex",

                        button {
                            class: "flex items-center gap-1.5 px-3 py-1.5 rounded-l-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50",
                            disabled: *is_exporting.read(),
                            onclick: move |_| {
                                if *is_exporting.read() {
                                    return;
                                }
                                is_exporting.set(true);
                                show_export_menu.set(false);
                                export_status.set("Exporting PDF...".to_string());

                                spawn(async move {
                                    let source = CHART_SOURCE.read().clone();
                                    let export_result = tokio::task::spawn_blocking(move || {
                                        let mut manager = ChartLayoutManager::new().map_err(|e| {
                                            format!("Failed to initialize chart layout manager: {e}")
                                        })?;
                                        manager
                                            .parse_and_layout(&source, 595.0, false)
                                            .map_err(|e| format!("Failed to layout chart: {e}"))?;
                                        manager.export_pdf_bytes()
                                    })
                                    .await;

                                    let pdf_bytes = match export_result {
                                        Ok(Ok(bytes)) => bytes,
                                        Ok(Err(err)) => {
                                            is_exporting.set(false);
                                            export_status.set(err);
                                            return;
                                        }
                                        Err(err) => {
                                            is_exporting.set(false);
                                            export_status.set(format!("Export task failed: {err}"));
                                            return;
                                        }
                                    };

                                    let save = rfd::AsyncFileDialog::new()
                                        .add_filter("PDF", &["pdf"])
                                        .set_file_name("chart-export.pdf")
                                        .save_file()
                                        .await;

                                    let Some(file) = save else {
                                        is_exporting.set(false);
                                        export_status.set("Export canceled".to_string());
                                        return;
                                    };

                                    match tokio::fs::write(file.path(), pdf_bytes).await {
                                        Ok(_) => {
                                            export_status
                                                .set(format!("Exported: {}", file.path().display()));
                                        }
                                        Err(err) => {
                                            export_status
                                                .set(format!("Failed to write PDF to disk: {err}"));
                                        }
                                    }

                                    is_exporting.set(false);
                                });
                            },
                            if *is_exporting.read() { "Exporting..." } else { "Export" }
                        }

                        button {
                            class: "flex items-center px-1.5 py-1.5 rounded-r-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors border-l border-primary-foreground/20 disabled:opacity-50",
                            disabled: *is_exporting.read(),
                            onclick: move |_| {
                                let current = *show_export_menu.read();
                                show_export_menu.set(!current);
                            },
                            "▾"
                        }
                    }

                    if *show_export_menu.read() {
                        div {
                            class: "absolute top-full right-0 mt-1 bg-popover border border-border rounded-md shadow-lg z-50 min-w-[120px]",
                            button {
                                class: "w-full flex items-center gap-2 px-3 py-2 text-xs text-left hover:bg-accent transition-colors rounded-t-md",
                                onclick: move |_| {
                                    if *is_exporting.read() {
                                        return;
                                    }
                                    is_exporting.set(true);
                                    show_export_menu.set(false);
                                    export_status.set("Exporting PDF...".to_string());

                                    spawn(async move {
                                        let source = CHART_SOURCE.read().clone();
                                        let export_result = tokio::task::spawn_blocking(move || {
                                            let mut manager = ChartLayoutManager::new().map_err(|e| {
                                                format!("Failed to initialize chart layout manager: {e}")
                                            })?;
                                            manager
                                                .parse_and_layout(&source, 595.0, false)
                                                .map_err(|e| format!("Failed to layout chart: {e}"))?;
                                            manager.export_pdf_bytes()
                                        })
                                        .await;

                                        let pdf_bytes = match export_result {
                                            Ok(Ok(bytes)) => bytes,
                                            Ok(Err(err)) => {
                                                is_exporting.set(false);
                                                export_status.set(err);
                                                return;
                                            }
                                            Err(err) => {
                                                is_exporting.set(false);
                                                export_status.set(format!("Export task failed: {err}"));
                                                return;
                                            }
                                        };

                                        let save = rfd::AsyncFileDialog::new()
                                            .add_filter("PDF", &["pdf"])
                                            .set_file_name("chart-export.pdf")
                                            .save_file()
                                            .await;

                                        let Some(file) = save else {
                                            is_exporting.set(false);
                                            export_status.set("Export canceled".to_string());
                                            return;
                                        };

                                        match tokio::fs::write(file.path(), pdf_bytes).await {
                                            Ok(_) => export_status
                                                .set(format!("Exported: {}", file.path().display())),
                                            Err(err) => export_status
                                                .set(format!("Failed to write PDF to disk: {err}")),
                                        }

                                        is_exporting.set(false);
                                    });
                                },
                                "PDF"
                            }

                            button {
                                class: "w-full flex items-center gap-2 px-3 py-2 text-xs text-left hover:bg-accent transition-colors rounded-b-md",
                                onclick: move |_| {
                                    if *is_exporting.read() {
                                        return;
                                    }
                                    is_exporting.set(true);
                                    show_export_menu.set(false);
                                    export_status.set("Exporting SVG...".to_string());

                                    spawn(async move {
                                        let source = CHART_SOURCE.read().clone();
                                        let export_result = tokio::task::spawn_blocking(move || {
                                            let mut manager = ChartLayoutManager::new().map_err(|e| {
                                                format!("Failed to initialize chart layout manager: {e}")
                                            })?;
                                            manager
                                                .parse_and_layout(&source, 595.0, false)
                                                .map_err(|e| format!("Failed to layout chart: {e}"))?;
                                            let pages = manager.export_svg_pages()?;

                                            if pages.len() == 1 {
                                                let svg = pages.into_iter().next().unwrap().into_bytes();
                                                Ok::<(Vec<u8>, Vec<&'static str>, PathBuf), String>(
                                                    (svg, vec!["svg"], PathBuf::from("chart-export.svg")),
                                                )
                                            } else {
                                                let zip = create_svg_zip(&pages)?;
                                                Ok::<(Vec<u8>, Vec<&'static str>, PathBuf), String>(
                                                    (zip, vec!["zip"], PathBuf::from("chart-export-pages.zip")),
                                                )
                                            }
                                        })
                                        .await;

                                        let (bytes, filters, default_name) = match export_result {
                                            Ok(Ok(value)) => value,
                                            Ok(Err(err)) => {
                                                is_exporting.set(false);
                                                export_status.set(err);
                                                return;
                                            }
                                            Err(err) => {
                                                is_exporting.set(false);
                                                export_status.set(format!("Export task failed: {err}"));
                                                return;
                                            }
                                        };

                                        let mut dialog = rfd::AsyncFileDialog::new();
                                        dialog = match filters.as_slice() {
                                            ["svg"] => dialog
                                                .add_filter("SVG", &["svg"])
                                                .set_file_name("chart-export.svg"),
                                            _ => dialog
                                                .add_filter("ZIP", &["zip"])
                                                .set_file_name("chart-export-pages.zip"),
                                        };

                                        let save = dialog.save_file().await;
                                        let Some(file) = save else {
                                            is_exporting.set(false);
                                            export_status.set("Export canceled".to_string());
                                            return;
                                        };

                                        match tokio::fs::write(file.path(), bytes).await {
                                            Ok(_) => export_status
                                                .set(format!("Exported: {}", file.path().display())),
                                            Err(err) => export_status
                                                .set(format!("Failed to write export to disk: {err}")),
                                        }

                                        let _ = default_name;
                                        is_exporting.set(false);
                                    });
                                },
                                "SVG"
                            }
                        }
                    }
                }
            }

            div { class: "flex-1 min-h-0", ChartView {} }
        }
    }
}
