//! The `document` contract: frame's own document model ([`model`]) and the
//! typed edits over it ([`ops`]).

mod hug;
pub mod model;
pub mod ops;
pub mod shader_input;

pub use model::*;
pub use ops::{History, Op, OpError, Prop};
/// The motion contract nodes carry (`Node::motion`).
pub use motion_proto as motion;
pub use shader_input::{InputMap, ShaderInput, ShaderInputSource};
