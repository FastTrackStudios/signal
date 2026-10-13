# frame

A design and control-surface engine: a native document format, Figma import,
a vello renderer held to pixel parity with Figma, and, in progress, a runtime
for fully mapped, AI-designable control surfaces (see
[docs/control-surface-roadmap.md](docs/control-surface-roadmap.md)).

frame is an [architect](https://github.com/FastTrackStudios/architect)
consumer repo.

```
apps/frame/desktop       `frame`: live surface player (hot reload, mock engine)
apps/frame/mcp           `frame-mcp`: MCP server — let a model design surfaces
apps/frame/cli           frame-cli: render_surface, export_markup, parity/diagnostic tools
crates/frame             facade crate: what apps and consumers depend on
features/document        model + ops/history (proto), Loro replica, .frame bundle (zip), .frame markup (text)
features/figma           Figma import only (figma-import; figma-model/rest are its parsers)
features/render          Scene → paint primitives → vello / SVG; tests/parity
features/surface         catalog/controls/bindings (proto), mock engine, runtime, widgets
features/design          lint + tool session (the AI authoring surface)
xtask                    cargo xtask: parity harness + scripts/
```

## Develop

```sh
nix develop          # GUI libs + Vulkan loader for the wgpu parity renders
just check
just test
just parity          # Figma parity suite (features/render/tests/parity)
just scaffold-feature <name>

cargo run --release -p frame-desktop                         # play the generated demo surface
cargo run --release -p frame-desktop -- examples/surfaces/demo.frame   # hot-reloads on save
claude mcp add frame -- $PWD/target/release/frame-mcp --file $PWD/stage.frame
```
