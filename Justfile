# signal workspace recipes
# Run commands: just <recipe-name>

# Default: serve the desktop app
default: dx

# ── Desktop App ──────────────────────────────────────────────────────────

# Serve the dioxus desktop app (hot-reload)
dx *args: tailwind
    cd apps/desktop && dx serve {{args}}

# Build the desktop app for release
dx-build: tailwind
    cd apps/desktop && dx build --release --platform desktop

# Build Tailwind CSS (v4)
tailwind:
    cd apps/desktop && tailwindcss -i ./input.css -o ./assets/tailwind.css --minify

# Watch Tailwind CSS for changes (run alongside dx serve)
tailwind-watch:
    cd apps/desktop && tailwindcss -i ./input.css -o ./assets/tailwind.css --watch --minify

# ── Build ────────────────────────────────────────────────────────────────

# Check all crates compile
check:
    cargo check --workspace

# Build all crates
build: tailwind
    cargo build --workspace

# Run tests
test:
    cargo test --workspace

# ── CLI ──────────────────────────────────────────────────────────────────

# Run the signal CLI
cli *args:
    cargo run -p signal-cli -- {{args}}

# Release build
release: tailwind
    cargo build --release

# ── Aliases ──────────────────────────────────────────────────────────────

alias c := check
alias b := build
alias t := test
