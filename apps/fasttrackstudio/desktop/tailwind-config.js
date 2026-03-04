module.exports = {
  content: [
    "./src/**/*.{rs,html,css}",
    // Session UI components
    "../../../crates/session/session-ui/src/**/*.rs",
    // Signal UI components
    "../../../crates/signal/signal-ui/src/**/*.rs",
    // Keyflow UI (chart editor) — sibling repo
    "../../../../keyflow/crates/keyflow-ui/src/**/*.rs",
    // Dock layout system — sibling repo
    "../../../../dock-dioxus/crates/dock-dioxus/src/**/*.rs",
    // DAW UI and audio controls — sibling repo
    "../../../../daw/crates/daw-ui/src/**/*.rs",
    "../../../../daw/crates/audio-controls/src/**/*.rs",
    // Input actions UI — sibling repo
    "../../../../input_actions/crates/input-dioxus/src/**/*.rs",
    // Lumen-blocks shared components
    "../../../reference/lumen-blocks/blocks/src/**/*.rs",
  ],
  theme: {
    extend: {},
  },
  plugins: [],
};
