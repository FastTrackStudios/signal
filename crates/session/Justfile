# session workspace recipes
# Run commands: just <recipe-name>

# Default — show recipes
_default:
    @just --list

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

# ── Desktop App ──────────────────────────────────────────────────────────

# Run the Dioxus desktop app with hot-reload (dx serve)
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

# ── Web App ─────────────────────────────────────────────────────────────

# Build Tailwind CSS for web app (v4)
tailwind-web:
    cd apps/web && tailwindcss -i ./input.css -o ./assets/tailwind.css --minify

# Build the web app (WASM) for release
web-build: tailwind-web
    cd apps/web && dx build --release --platform web

# Build the web app for development
web-dev: tailwind-web
    cd apps/web && dx build --platform web

# ── CLI ──────────────────────────────────────────────────────────────────

# Run the session CLI
cli *ARGS:
    cargo run -p session-cli -- {{ARGS}}

# ── Aliases ──────────────────────────────────────────────────────────────

alias c := check
alias b := build
alias t := test
