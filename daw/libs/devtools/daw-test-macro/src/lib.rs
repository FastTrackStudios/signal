//! Proc-macro crate for the `#[reaper_test]` attribute.
//!
//! Transforms:
//! ```ignore
//! #[reaper_test]
//! async fn test_something(ctx: &ReaperTestContext) -> Result<()> {
//!     let project = ctx.project();
//!     // ...
//!     Ok(())
//! }
//! ```
//!
//! Into:
//! ```ignore
//! #[test]
//! #[ignore = "reaper-test: run with `cargo xtask reaper-test`"]
//! fn test_something() -> eyre::Result<()> {
//!     reaper_test::run_reaper_test(
//!         "test_something",
//!         false,
//!         |ctx| Box::pin(async move {
//!             let project = ctx.project();
//!             // ...
//!             Ok(())
//!         }),
//!     )
//! }
//! ```
//!
//! Multi-instance tests:
//! ```ignore
//! #[reaper_test(instances("master", "follower"))]
//! async fn position_sync(ctx: &MultiDawTestContext) -> Result<()> {
//!     let master = ctx.by_label("master");
//!     let follower = ctx.by_label("follower");
//!     // ...
//!     Ok(())
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// Attribute macro for REAPER integration tests.
///
/// Automatically adds `#[test]` and `#[ignore]`, wraps the test body
/// in `reaper_test::run_reaper_test()` which handles connection, project
/// tab assignment, and cleanup.
///
/// # Arguments
///
/// - `isolated` — optional, gives this test its own REAPER project tab.
///   Without `isolated`, the test joins a batch of tests sharing one tab.
///   Use `isolated` for tests that do destructive operations like
///   removing all tracks or needing a guaranteed-clean project.
///
/// - `instances("label1", "label2", ...)` — spawns multiple REAPER instances
///   and provides a `MultiDawTestContext` instead of `ReaperTestContext`.
///   Each label becomes a `DawInstanceConfig` with that label.
///
/// # Examples
///
/// ```ignore
/// // Joins a batch — shares a project tab with other tests
/// #[reaper_test]
/// async fn test_add_fx(ctx: &ReaperTestContext) -> Result<()> {
///     let track = ctx.project().tracks().add("My Track", None).await?;
///     Ok(())
/// }
///
/// // Gets its own project tab
/// #[reaper_test(isolated)]
/// async fn test_destructive(ctx: &ReaperTestContext) -> Result<()> {
///     ctx.project().tracks().remove_all().await?;
///     Ok(())
/// }
///
/// // Multi-instance test — spawns 2 REAPERs
/// #[reaper_test(instances("master", "follower"))]
/// async fn position_sync(ctx: &MultiDawTestContext) -> Result<()> {
///     let master = ctx.by_label("master");
///     let follower = ctx.by_label("follower");
///     ctx.connect_sync_peers("FTS_SYNC_EXT").await?;
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn daw_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    daw_test_impl(attr, item)
}

/// Alias for [`daw_test`] — kept for source compatibility with existing
/// `#[reaper_test]` call sites. Forwards to the same implementation.
#[proc_macro_attribute]
pub fn reaper_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    daw_test_impl(attr, item)
}

fn daw_test_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    let mut isolated = false;
    let mut instance_labels: Option<Vec<String>> = None;

    if !attr.is_empty() {
        let attr_str = attr.to_string();
        let trimmed = attr_str.trim();

        if trimmed == "isolated" {
            isolated = true;
        } else if trimmed.starts_with("instances") {
            // Parse instances("label1", "label2", ...)
            match parse_instances_attr(trimmed) {
                Ok(labels) => {
                    if labels.len() < 2 {
                        return syn::Error::new(
                            proc_macro2::Span::call_site(),
                            "instances() requires at least 2 labels",
                        )
                        .to_compile_error()
                        .into();
                    }
                    instance_labels = Some(labels);
                }
                Err(msg) => {
                    return syn::Error::new(proc_macro2::Span::call_site(), msg)
                        .to_compile_error()
                        .into();
                }
            }
        } else {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "expected `isolated` or `instances(\"label1\", \"label2\", ...)`",
            )
            .to_compile_error()
            .into();
        }
    }

    let fn_name = &input_fn.sig.ident;
    let fn_block = &input_fn.block;
    let fn_vis = &input_fn.vis;
    let fn_attrs = &input_fn.attrs;

    // Validate: must be async
    if input_fn.sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            input_fn.sig.fn_token,
            "reaper_test functions must be async",
        )
        .to_compile_error()
        .into();
    }

    // Strip the ctx parameter — the closure will provide it.
    let ctx_ident = if input_fn.sig.inputs.len() == 1 {
        let arg = input_fn.sig.inputs.first().unwrap();
        match arg {
            syn::FnArg::Typed(pat_type) => match pat_type.pat.as_ref() {
                syn::Pat::Ident(pat_ident) => pat_ident.ident.clone(),
                _ => {
                    return syn::Error::new_spanned(
                        &pat_type.pat,
                        "expected a simple identifier for the context parameter",
                    )
                    .to_compile_error()
                    .into();
                }
            },
            _ => {
                return syn::Error::new_spanned(arg, "expected a typed parameter, not self")
                    .to_compile_error()
                    .into();
            }
        }
    } else if input_fn.sig.inputs.is_empty() {
        syn::Ident::new("_ctx", proc_macro2::Span::call_site())
    } else {
        return syn::Error::new_spanned(
            &input_fn.sig.inputs,
            "reaper_test functions must have zero or one parameter",
        )
        .to_compile_error()
        .into();
    };

    let fn_name_str = fn_name.to_string();

    if let Some(labels) = instance_labels {
        // Multi-instance test
        // Each instance gets:
        // - DISPLAY="": suppresses GUI windows (REAPER/SWELL works headless on Linux)
        // - FTS_SYNC_NO_MDNS: prevents mDNS cross-talk with other REAPER instances
        // - FTS_SYNC_NO_LINK: prevents Link cross-talk on shared loopback
        // - -cfgfile: uses FTS config dir (has audiodriver=2 for headless playback)
        let label_tokens: Vec<_> = labels
            .iter()
            .map(|l| {
                let socket = format!("/tmp/fts-daw-test-{l}.sock");
                quote! {
                    daw::test::DawInstanceConfig::new(#l)
                        .with_env("DISPLAY", "")
                        .with_env("FTS_SYNC_NO_LINK", "1")
                        .with_env("FTS_SYNC_ENABLED", "1")
                        .with_fts_config()
                        .with_socket(#socket)
                }
            })
            .collect();

        let output = quote! {
            #(#fn_attrs)*
            #[test]
            #[ignore = "reaper-test: run with `cargo xtask reaper-test`"]
            #fn_vis fn #fn_name() -> eyre::Result<()> {
                daw::test::run_multi_daw_test(
                    #fn_name_str,
                    vec![#(#label_tokens),*],
                    |#ctx_ident| Box::pin(async move #fn_block),
                )
            }
        };

        output.into()
    } else {
        // Single-instance test
        let output = quote! {
            #(#fn_attrs)*
            #[test]
            #[ignore = "reaper-test: run with `cargo xtask reaper-test`"]
            #fn_vis fn #fn_name() -> eyre::Result<()> {
                daw::test::run_daw_test(
                    #fn_name_str,
                    #isolated,
                    |#ctx_ident| Box::pin(async move #fn_block),
                )
            }
        };

        output.into()
    }
}

/// Parse `instances("label1", "label2", ...)` from the attribute string.
fn parse_instances_attr(s: &str) -> Result<Vec<String>, String> {
    let s = s.trim();
    let inner = s
        .strip_prefix("instances")
        .ok_or("expected 'instances(...)'")?
        .trim();
    let inner = inner
        .strip_prefix('(')
        .ok_or("expected '(' after 'instances'")?;
    let inner = inner
        .strip_suffix(')')
        .ok_or("expected ')' at end of instances(...)")?;

    let mut labels = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        // Strip quotes
        let label = part
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or_else(|| format!("expected quoted string, got: {part}"))?;
        if label.is_empty() {
            return Err("instance labels must not be empty".to_string());
        }
        labels.push(label.to_string());
    }

    Ok(labels)
}
