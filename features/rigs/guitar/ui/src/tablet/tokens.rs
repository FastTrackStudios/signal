//! The tablet layout's tokens: the rig frame's, shared by every rig
//! (`signal-rig-shell`). A colour is changed there or nowhere.

pub use signal_rig_shell::tokens::*;

// The header rows line up with the macro bar.
const _: () = assert!(HEADER_H == super::macros::MACRO_BAR_H);
