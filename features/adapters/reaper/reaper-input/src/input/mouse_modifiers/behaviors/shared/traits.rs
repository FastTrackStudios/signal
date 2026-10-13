//! Traits for mouse modifier behaviors

/// Trait for types that can convert to/from behavior IDs
///
/// This allows for trait bounds in generic functions that work with any behavior type.
pub trait BehaviorId {
    /// Convert from behavior ID to enum variant
    fn from_behavior_id(behavior_id: u32) -> Self
    where
        Self: Sized;

    /// Convert enum variant to behavior ID
    fn to_behavior_id(&self) -> u32;
}

/// Trait for behavior enums that have display names
pub trait BehaviorDisplay: BehaviorId + std::fmt::Debug {
    /// Get the display name for this behavior
    fn display_name(&self) -> &'static str;

    /// Get the behavior ID string format (e.g., "1 m", "13 m")
    fn behavior_id_string(&self) -> String {
        format!("{} m", self.to_behavior_id())
    }
}

/// Object-safe trait for mouse modifier behaviors
///
/// This trait is object-safe and can be used with trait objects (`&dyn MouseBehavior`).
/// It combines the functionality of `BehaviorId` and `BehaviorDisplay` in an object-safe way.
pub trait MouseBehavior: std::fmt::Debug {
    /// Get the behavior ID
    fn behavior_id(&self) -> u32;

    /// Get the display name for this behavior
    fn display_name(&self) -> &'static str;

    /// Get the behavior ID string format (e.g., "1 m", "13 m")
    fn behavior_id_string(&self) -> String {
        format!("{} m", self.behavior_id())
    }
}

// Blanket implementation: any type that implements BehaviorDisplay also implements MouseBehavior
impl<T: BehaviorDisplay> MouseBehavior for T {
    fn behavior_id(&self) -> u32 {
        self.to_behavior_id()
    }

    fn display_name(&self) -> &'static str {
        BehaviorDisplay::display_name(self)
    }

    fn behavior_id_string(&self) -> String {
        BehaviorDisplay::behavior_id_string(self)
    }
}
