//! Action registration — types + events + service trait.

mod event;
mod service;
mod types;

pub use event::ActionEvent;
pub use service::*;
pub use types::{
    ActionExecutionResult, ActionInfo, ActionListFilter, ActionListRequest, ActionListResponse,
    ActionOrigin, ActionSection,
};
