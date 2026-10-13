//! Input — types + service trait.
//!
//! Streaming subscription (`subscribe_input`) retired with the
//! architect::rpc port; sibling-trait territory.

mod service;
mod types;

pub use service::*;
pub use types::{
    InputContext, InputEvent, KeyCode, KeyEvent, KeyFilter, KeyModifiers, KeyMsgKind, KeyPattern,
};
