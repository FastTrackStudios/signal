//! The pack CLI standalone (`fts signal pack …` without fts-cli):
//!   cargo run -p signal-sampler --release --example signalpack -- build <dir> <out.signalpack>
fn main() -> eyre::Result<()> {
    signal_sampler::pack_cli::cli_main(std::env::args_os().skip(1))
}
