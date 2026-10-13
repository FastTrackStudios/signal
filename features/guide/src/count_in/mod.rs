//! Count-in pattern calculations and state management
//!
//! This module handles all logic related to count-in patterns for sections and SONGEND markers.

mod calculator;
mod pattern;
mod state;

pub use calculator::CountInCalculator;
pub use pattern::CountInPattern;
pub use state::CountInState;
