//! `daw` — thin binary over `daw::cli` (build with `--features cli`).
//!
//! The same command tree is mounted at `fts daw <...>` by the unified
//! `fts` CLI; this standalone binary exists for muscle memory and for the
//! CLI integration tests (`CARGO_BIN_EXE_daw`).

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    daw::cli::cli_main(std::env::args()).await
}
